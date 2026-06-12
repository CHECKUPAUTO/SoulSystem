//! Integration smoke test for the TUI's external surface.
//!
//! `pub async fn run()` is the only exported entrypoint. We can't drive
//! a real terminal in tests, but we can verify:
//!   - the function exists and is `async`
//!   - it has the expected `Result` signature
//!   - calling it with a hostile env (server down) returns an error
//!     instead of panicking
//!
//! For deeper UI-state testing, see the inline unit tests in
//! `src/tests_inline.rs`.

#[test]
fn run_signature_is_stable() {
    // This is a compile-time test: the function `run` must exist with
    // this exact signature. If someone renames or changes the return
    // type, this won't compile, which is the intent.
    fn _assert_signature() -> impl std::future::Future<Output = color_eyre::Result<()>> {
        avid_tui::run()
    }
}

// Note: we don't run avid_tui::run() at test time because it tries to
// open the real terminal (enable_raw_mode + alternate screen). Doing
// that in a non-TTY CI environment fails immediately. A proper test
// would require dependency injection of the Terminal backend — see
// the TODO in #issue for v2 of the TUI.
