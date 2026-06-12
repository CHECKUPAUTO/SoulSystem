use leptos::prelude::*;
use shared::{User, ProjectSummary, ProjectDetail, Message};
use serde_json::json;
use gloo_net::http::Request;
use uuid::Uuid;
use futures::Stream;

pub async fn login(email: String, password: String) -> Result<User, String> {
    let resp = Request::post("/api/auth/login")
        .json(&json!({ "email": email, "password": password })).map_err(|e| e.to_string())?
        .send().await.map_err(|e| e.to_string())?;

    if resp.ok() {
        let auth_resp: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        serde_json::from_value(auth_resp["user"].clone()).map_err(|e| e.to_string())
    } else {
        Err("Login failed".to_string())
    }
}

pub async fn list_projects() -> Result<Vec<ProjectSummary>, String> {
    let resp = Request::get("/api/projects")
        .send().await.map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err("Failed to load projects".to_string())
    }
}

pub async fn get_project(id: Uuid) -> Result<ProjectDetail, String> {
    let resp = Request::get(&format!("/api/projects/{}", id))
        .send().await.map_err(|e| e.to_string())?;
    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err("Failed to load project".to_string())
    }
}

pub async fn get_conversation_messages(id: Uuid) -> Result<Vec<Message>, String> {
    let resp = Request::get(&format!("/api/conversations/{}", id))
        .send().await.map_err(|e| e.to_string())?;
    if resp.ok() {
        let data: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        serde_json::from_value(data["messages"].clone()).map_err(|e| e.to_string())
    } else {
        Err("Failed to load messages".to_string())
    }
}

pub enum SseEvent {
    Delta(String),
    Meta(Uuid),
    Done,
    Error(String),
}

pub async fn send_message_stream(conv_id: Uuid, content: String) -> impl Stream<Item = SseEvent> {
    let (tx, rx) = futures::channel::mpsc::unbounded();

    leptos::task::spawn_local(async move {
        let resp = Request::post(&format!("/api/conversations/{}/messages", conv_id))
            .json(&json!({ "content": content }))
            .unwrap()
            .send().await;

        if let Ok(resp) = resp {
             let body = resp.binary().await.unwrap_or_default();
             let text = String::from_utf8_lossy(&body);
             for line in text.lines() {
                 if line.starts_with("data: ") {
                     let data = &line[6..];
                     if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                         tx.unbounded_send(SseEvent::Delta(val.as_str().unwrap_or_default().to_string())).ok();
                     }
                 }
             }
             tx.unbounded_send(SseEvent::Done).ok();
        }
    });

    rx
}
