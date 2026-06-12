//! Tests d'integration pour le streaming de commandes shell vers Telegram.

use soulsystem::bound_system::{BoundSystem, StreamMessage};

#[tokio::test]
async fn test_streaming_basic() {
    let bs = BoundSystem::new(vec!["echo".into()]).without_sandbox();
    let exec = bs.execute_streaming("echo hello").await.unwrap();
    assert!(exec.pid.is_some(), "Should capture PID");
    let mut rx = exec.rx;

    let mut lines = Vec::new();
    let mut exit_code = None;

    while let Some(msg) = rx.recv().await {
        match msg {
            StreamMessage::Line(line) => lines.push(line.content),
            StreamMessage::End(end) => {
                exit_code = Some(end.exit_code);
                break;
            }
            StreamMessage::Error(e) => panic!("Unexpected: {}", e),
        }
    }

    assert_eq!(lines, vec!["hello"]);
    assert_eq!(exit_code, Some(0));
}

#[tokio::test]
async fn test_streaming_multi_line() {
    let bs = BoundSystem::new(vec!["printf".into()]).without_sandbox();
    let exec = bs
        .execute_streaming("printf 'line1\\nline2\\nline3\\n'")
        .await
        .unwrap();
    let mut rx = exec.rx;

    let mut lines = Vec::new();
    let mut end_received = false;

    while let Some(msg) = rx.recv().await {
        match msg {
            StreamMessage::Line(line) => lines.push(line.content),
            StreamMessage::End(_) => {
                end_received = true;
                break;
            }
            StreamMessage::Error(e) => panic!("Unexpected: {}", e),
        }
    }

    assert!(end_received, "Should receive StreamEnd");
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "line1");
    assert_eq!(lines[1], "line2");
    assert_eq!(lines[2], "line3");
}

#[tokio::test]
async fn test_streaming_stderr() {
    let bs = BoundSystem::new(vec!["sh".into()]).without_sandbox();
    let exec = bs
        .execute_streaming("sh -c 'echo ok && echo err >&2'")
        .await
        .unwrap();
    let mut rx = exec.rx;

    let mut stdout_lines = Vec::new();
    let mut stderr_lines = Vec::new();

    while let Some(msg) = rx.recv().await {
        match msg {
            StreamMessage::Line(line) => {
                if line.is_stderr {
                    stderr_lines.push(line.content);
                } else {
                    stdout_lines.push(line.content);
                }
            }
            StreamMessage::End(_) => break,
            StreamMessage::Error(_) => {}
        }
    }

    assert_eq!(stdout_lines, vec!["ok"]);
    assert_eq!(stderr_lines, vec!["err"]);
}

#[tokio::test]
async fn test_streaming_rejected() {
    let bs = BoundSystem::new(vec![]).without_sandbox();
    let result = bs.execute_streaming("date").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("non autorisee"));
}
