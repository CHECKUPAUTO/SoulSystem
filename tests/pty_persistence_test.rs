//! Tests d'intégration pour la persistance PTY via tmux.
//!
//! Ces tests sont ignorés par défaut si tmux n'est pas disponible.

use soulsystem::pty_terminal::PtyTerminal;
use std::time::Duration;

/// Vérifie que tmux est disponible pour les tests de persistance.
fn tmux_available() -> bool {
    PtyTerminal::tmux_available()
}

#[test]
#[ignore = "Requiert tmux installé"]
fn test_tmux_session_create_and_destroy() {
    if !tmux_available() {
        eprintln!("tmux not available, skipping");
        return;
    }

    let session_name = "soulsystem_test_99999";
    let pty = PtyTerminal::with_tmux_session(session_name);
    assert!(pty.is_ok(), "Should create tmux session");
    let pty = pty.unwrap();
    assert!(pty.child_id().is_some());
    assert_eq!(pty.tmux_session_name(), Some(session_name.to_string()));

    // Clean up
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

#[test]
#[ignore = "Requiert tmux installé"]
fn test_tmux_session_write_and_read() {
    if !tmux_available() {
        eprintln!("tmux not available, skipping");
        return;
    }

    let session_name = "soulsystem_test_88888";

    // Kill existing session if any
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();

    let pty = PtyTerminal::with_tmux_session(session_name).unwrap();

    std::thread::sleep(Duration::from_millis(500));
    let _ = pty.read(); // drain

    pty.write("echo TMUX_TEST_MARKER\n").unwrap();
    std::thread::sleep(Duration::from_millis(500));

    let output = pty.read().unwrap();
    assert!(
        output.contains("TMUX_TEST_MARKER"),
        "Expected TMUX_TEST_MARKER in output, got: {}",
        output
    );

    // Clean up
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

#[test]
#[ignore = "Requiert tmux installé"]
fn test_tmux_session_reattach() {
    if !tmux_available() {
        eprintln!("tmux not available, skipping");
        return;
    }

    let session_name = "soulsystem_test_77777";

    // Kill existing
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();

    // Create first session
    let pty1 = PtyTerminal::with_tmux_session(session_name).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = pty1.read();
    pty1.write("echo PERSISTENT_DATA\n").unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let _ = pty1.read();
    // pty1 is dropped here but tmux session survives

    // Reattach (in real scenario this would be after restart)
    let pty2 = PtyTerminal::with_tmux_session(session_name).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    // Just verify we can interact
    pty2.write("echo REATTACH_OK\n").unwrap();
    std::thread::sleep(Duration::from_millis(500));

    let output = pty2.read().unwrap();
    assert!(
        output.contains("REATTACH_OK"),
        "Expected REATTACH_OK after reattach, got: {}",
        output
    );

    // Clean up
    let _ = std::process::Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

#[test]
fn test_list_soulsystem_sessions() {
    let sessions = PtyTerminal::list_soulsystem_sessions();
    assert!(
        sessions.is_ok(),
        "list_soulsystem_sessions should not panic"
    );
}

#[test]
fn test_save_and_restore_env_roundtrip() {
    let chat_id = 12345; // use a unique test id

    let save_result = PtyTerminal::save_env(chat_id);
    assert!(save_result.is_ok());

    let restore_result = PtyTerminal::restore_env(chat_id);
    assert!(restore_result.is_ok());
    let vars = restore_result.unwrap();

    // Should at least contain PATH and HOME
    assert!(vars.contains_key("PATH"), "Should contain PATH");
    assert!(vars.contains_key("HOME"), "Should contain HOME");
}
