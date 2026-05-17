//! Tests d'integration pour le terminal PTY persistant.

use soulsystem::pty_terminal::PtyTerminal;

#[test]
fn test_pty_create_and_destroy() {
    let pty = PtyTerminal::new();
    assert!(pty.is_ok(), "PTY creation should succeed");
}

#[test]
fn test_pty_write_and_read_output() {
    let pty = PtyTerminal::new().unwrap();

    // Drain initial bash output
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = pty.read();

    pty.write("echo HELLO_PTY_INTEGRATION_TEST\n").unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let output = pty.read().unwrap();
    assert!(
        output.contains("HELLO_PTY_INTEGRATION_TEST"),
        "Expected 'HELLO_PTY_INTEGRATION_TEST' in PTY output, got: '{}'",
        output
    );
}

#[test]
fn test_pty_clone_shares_state() {
    let pty = PtyTerminal::new().unwrap();

    // Drain initial bash output
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = pty.read();

    let pty2 = pty.clone();

    pty.write("echo SHARED\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));

    let output = pty2.read().unwrap();
    assert!(
        output.contains("SHARED"),
        "Cloned PTY should share state, got: '{}'",
        output
    );
}

#[test]
fn test_pty_multiple_commands() {
    let pty = PtyTerminal::new().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = pty.read(); // drain initial output

    pty.write("echo FIRST\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let _ = pty.read(); // consume

    pty.write("echo SECOND\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let output = pty.read().unwrap();

    assert!(
        output.contains("SECOND"),
        "Should find second command output, got: '{}'",
        output
    );
}

#[test]
fn test_pty_resize_no_panic() {
    let pty = PtyTerminal::new().unwrap();
    let result = pty.resize(50, 120);
    assert!(result.is_ok());
}
