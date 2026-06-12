use leptos::prelude::*;
use crate::api;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

#[component]
pub fn ProjectPage() -> impl IntoView {
    let params = use_params_map();
    let project_id = move || params.with(|p| p.get("project_id").and_then(|id| Uuid::parse_str(&id).ok()));

    let project = LocalResource::new(move || {
        let id = project_id();
        async move {
            match id {
                Some(id) => api::get_project(id).await.ok(),
                None => None,
            }
        }
    });

    view! {
        <div class="project-page">
            <Suspense fallback=|| view! { <p>"Loading project..."</p> }>
                {move || Suspend::new(async move {
                    project.await.map(|p| view! {
                        <div class="project-layout">
                            <div class="sidebar">
                                <h2>{p.name}</h2>
                                <ConversationList conversations=p.conversations />
                            </div>
                            <div class="chat-area">
                                <ChatArea />
                            </div>
                        </div>
                    })
                })}
            </Suspense>
        </div>
    }
}

#[component]
fn ConversationList(conversations: Vec<shared::Conversation>) -> impl IntoView {
    view! {
        <ul>
            {conversations.into_iter().map(|c| view! {
                <li><a href=format!("/p/{}/c/{}", c.project_id, c.id)>{c.title}</a></li>
            }).collect::<Vec<_>>()}
        </ul>
    }
}

#[component]
fn ChatArea() -> impl IntoView {
    let params = use_params_map();
    let conv_id = move || params.with(|p| p.get("cid").and_then(|id| Uuid::parse_str(&id).ok()));

    let messages = LocalResource::new(move || {
        let id = conv_id();
        async move {
            match id {
                Some(id) => api::get_conversation_messages(id).await.unwrap_or_default(),
                None => vec![],
            }
        }
    });

    let pending_assistant = RwSignal::new(String::new());
    let is_streaming = RwSignal::new(false);

    let on_send = move |content: String| {
        if let Some(id) = conv_id() {
            is_streaming.set(true);
            pending_assistant.set(String::new());
            leptos::task::spawn_local(async move {
                let mut stream = api::send_message_stream(id, content).await;
                use futures::StreamExt;
                while let Some(event) = stream.next().await {
                    match event {
                        api::SseEvent::Delta(d) => pending_assistant.update(|p| p.push_str(&d)),
                        api::SseEvent::Done => break,
                        _ => {}
                    }
                }
                is_streaming.set(false);
                messages.refetch();
            });
        }
    };

    view! {
        <div class="chat-box">
            <Suspense fallback=|| view! { <p>"Loading messages..."</p> }>
                {move || Suspend::new(async move {
                    let msgs = messages.await;
                    view! {
                        <div class="messages">
                            {msgs.into_iter().map(|m| view! {
                                <div class=format!("message {}", m.role)>
                                    <p>{m.content}</p>
                                </div>
                            }).collect::<Vec<_>>()}
                            {move || is_streaming.get().then(|| view! {
                                <div class="message assistant streaming">
                                    <p>{pending_assistant.get()}</p>
                                </div>
                            })}
                        </div>
                    }
                })}
            </Suspense>
            <MessageInput on_send=Callback::new(on_send) disabled=is_streaming />
        </div>
    }
}

#[component]
fn MessageInput(on_send: Callback<String>, disabled: RwSignal<bool>) -> impl IntoView {
    let input = RwSignal::new(String::new());
    view! {
        <div class="input-area">
            <textarea
                on:input=move |ev| input.set(event_target_value(&ev))
                prop:value=input
                disabled=move || disabled.get()
            ></textarea>
            <button
                on:click=move |_| {
                    on_send.run(input.get_untracked());
                    input.set(String::new());
                }
                disabled=move || disabled.get()
            >"Send"</button>
        </div>
    }
}
