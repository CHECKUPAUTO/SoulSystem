use axum::{
    extract::{State, Path},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::stream::Stream;
use std::{convert::Infallible, time::Duration};
use uuid::Uuid;
use shared::Message;
use crate::{AppState, middleware_auth::AuthUser, error::AppError, context::plan_context, streaming::StreamEvent, llm::LlmStreamEvent};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

pub async fn send_message(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    Path(conv_id): Path<Uuid>,
    Json(body): Json<SendMessageRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    // 1. Verify ownership (simplified)
    let project_id = sqlx::query_as::<_, (Uuid,)>(
        "SELECT project_id FROM conversations WHERE id = $1"
    )
    .bind(conv_id)
    .fetch_one(&state.db)
    .await?.0;

    let owner_id = sqlx::query_as::<_, (Uuid,)>(
        "SELECT user_id FROM projects WHERE id = $1"
    )
    .bind(project_id)
    .fetch_one(&state.db)
    .await?.0;

    if owner_id != user.id {
        return Err(AppError::Forbidden);
    }

    // 2. Load context
    let project = sqlx::query_as::<_, shared::Project>(
        "SELECT * FROM projects WHERE id = $1"
    ).bind(project_id).fetch_one(&state.db).await?;

    let knowledge = sqlx::query_as::<_, shared::ProjectKnowledge>(
        "SELECT * FROM project_knowledge WHERE project_id = $1"
    ).bind(project_id).fetch_all(&state.db).await?;

    let knowledge_contents: Vec<String> = knowledge.iter().map(|k| k.content.clone()).collect();

    let history = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at"
    ).bind(conv_id).fetch_all(&state.db).await?;

    // 3. Persist user message
    sqlx::query(
        "INSERT INTO messages (conversation_id, role, content, status) VALUES ($1, 'user', $2, 'complete')"
    )
    .bind(conv_id)
    .bind(&body.content)
    .execute(&state.db)
    .await?;

    // 4. Create assistant message
    let assistant_msg_id = sqlx::query_as::<_, (Uuid,)>(
        "INSERT INTO messages (conversation_id, role, content, status) VALUES ($1, 'assistant', '', 'streaming') RETURNING id"
    )
    .bind(conv_id)
    .fetch_one(&state.db)
    .await?.0;

    // 5. Plan context
    let plan = plan_context(
        &project,
        &knowledge,
        &knowledge_contents,
        &history,
        &body.content,
        state.budget_config.reserved_response,
        state.budget_config.reserved_meta,
    )?;

    // 6. Register broker
    let (tx, cancel) = state.broker.register(assistant_msg_id).await;

    // 7. Spawn generation
    let state_c = state.clone();
    let assistant_msg_id_c = assistant_msg_id;
    tokio::spawn(async move {
        let _ = tx.send(StreamEvent::Meta { assistant_message_id: assistant_msg_id_c });

        let mut full = String::new();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;

        let stream_res = state_c.llm.stream(&project.model, &plan).await;

        match stream_res {
            Ok(mut stream) => {
                use futures::StreamExt;
                while let Some(event) = stream.next().await {
                    if cancel.is_cancelled() { break; }
                    match event {
                        Ok(LlmStreamEvent::Delta(delta)) => {
                            full.push_str(&delta);
                            let _ = tx.send(StreamEvent::Delta(delta));
                        }
                        Ok(LlmStreamEvent::Usage { input_tokens: i, output_tokens: o }) => {
                            input_tokens = i;
                            output_tokens = o;
                        }
                        Ok(LlmStreamEvent::Done) => break,
                        Err(e) => {
                            let _ = tx.send(StreamEvent::Error(e.to_string()));
                            break;
                        }
                    }
                }

                let status = if cancel.is_cancelled() { "cancelled" } else { "complete" };
                sqlx::query(
                    "UPDATE messages SET content = $1, status = $2, input_tokens = $3, output_tokens = $4, completed_at = NOW() WHERE id = $5"
                )
                .bind(full)
                .bind(status)
                .bind(input_tokens as i32)
                .bind(output_tokens as i32)
                .bind(assistant_msg_id_c)
                .execute(&state_c.db)
                .await.ok();

                if !cancel.is_cancelled() {
                    let _ = tx.send(StreamEvent::Usage { input_tokens, output_tokens });
                }
                let _ = tx.send(StreamEvent::Done);
            }
            Err(e) => {
                let err_msg = e.to_string();
                sqlx::query(
                    "UPDATE messages SET status = 'error', error = $1, completed_at = NOW() WHERE id = $2"
                )
                .bind(&err_msg)
                .bind(assistant_msg_id_c)
                .execute(&state_c.db)
                .await.ok();
                let _ = tx.send(StreamEvent::Error(err_msg));
            }
        }
        state_c.broker.cleanup(assistant_msg_id_c).await;
    });

    // 8. Build SSE response
    let rx = state.broker.subscribe(assistant_msg_id).await.unwrap();
    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(StreamEvent::Meta { assistant_message_id }) => {
                    yield Ok(Event::default().event("meta").data(serde_json::json!({"assistant_message_id": assistant_message_id}).to_string()));
                }
                Ok(StreamEvent::Delta(s)) => {
                    yield Ok(Event::default().event("delta").data(s));
                }
                Ok(StreamEvent::Usage { input_tokens, output_tokens }) => {
                    yield Ok(Event::default().event("usage").data(serde_json::json!({"input_tokens": input_tokens, "output_tokens": output_tokens}).to_string()));
                }
                Ok(StreamEvent::Done) => {
                    yield Ok(Event::default().event("done").data(""));
                    break;
                }
                Ok(StreamEvent::Error(e)) => {
                    yield Ok(Event::default().event("error").data(e));
                    break;
                }
                Err(_) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
