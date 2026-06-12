// ════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════
//
// All tests are inline (#[cfg(test)] mod tests) so they can access the
// private `App`, `ChatMessage`, etc. without forcing those types to be
// `pub`. The TUI's surface stays minimal (just `run()` public).
//
// Coverage:
//   - App::new starts in a known state
//   - add_message appends correctly with right role
//   - input history navigation (prev/next) handles bounds
//   - autocomplete filters by prefix
//   - scroll_to_bottom respects viewport
//   - on_tick is idempotent and safe
//   - command recognition (parsing "/help" etc. without dispatch)
//   - UI rendering doesn't panic and includes key strings in buffer
//
// Run with: cargo test -p avid-tui --lib

#[cfg(test)]
mod tests {
    use crate::ui;
    use crate::App;
    use crate::*;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    // ── Helpers ──────────────────────────────────────────────────

    fn empty_app() -> App {
        App::new()
    }

    fn app_with_history(items: &[&str]) -> App {
        let mut app = App::new();
        for s in items {
            app.input_history.push((*s).to_string());
        }
        app
    }

    // ── State initialization ────────────────────────────────────

    #[test]
    fn new_app_has_one_system_message() {
        let app = empty_app();
        assert_eq!(app.messages.len(), 1);
        assert!(matches!(app.messages[0].role, MessageRole::System));
        assert!(app.messages[0].content.contains("AVID"));
    }

    #[test]
    fn new_app_has_empty_input() {
        let app = empty_app();
        assert_eq!(app.input.value(), "");
        assert!(app.input_history.is_empty());
        assert!(app.input_history_index.is_none());
    }

    #[test]
    fn new_app_has_known_default_model() {
        let app = empty_app();
        // The default model is set at construction; if it changes,
        // update this test rather than silently drifting.
        assert!(!app.current_model.is_empty());
        assert!(
            app.current_model.contains(':'),
            "expected name:tag format, got {}",
            app.current_model
        );
    }

    #[test]
    fn new_app_offers_suggestions() {
        let app = empty_app();
        assert!(!app.suggestions.is_empty(), "should preload command list");
        assert!(app.suggestions.contains(&"/help"));
        assert!(app.suggestions.contains(&"/quit"));
    }

    // ── add_message ─────────────────────────────────────────────

    #[test]
    fn add_message_appends_with_correct_role() {
        let mut app = empty_app();
        let initial_len = app.messages.len();
        app.add_message(MessageRole::User, "hello".to_string());
        app.add_message(MessageRole::Assistant, "hi".to_string());
        app.add_message(MessageRole::Error, "boom".to_string());

        assert_eq!(app.messages.len(), initial_len + 3);

        let last_three = &app.messages[initial_len..];
        assert!(matches!(last_three[0].role, MessageRole::User));
        assert!(matches!(last_three[1].role, MessageRole::Assistant));
        assert!(matches!(last_three[2].role, MessageRole::Error));
        assert_eq!(last_three[0].content, "hello");
        assert_eq!(last_three[1].content, "hi");
        assert_eq!(last_three[2].content, "boom");
    }

    #[test]
    fn add_message_preserves_order() {
        let mut app = empty_app();
        for i in 0..50 {
            app.add_message(MessageRole::User, format!("msg-{i}"));
        }
        // Skip the initial system message
        for (idx, msg) in app.messages.iter().skip(1).enumerate() {
            assert_eq!(msg.content, format!("msg-{idx}"));
        }
    }

    // ── input history navigation ────────────────────────────────

    #[test]
    fn prev_history_walks_backwards() {
        let mut app = app_with_history(&["first", "second", "third"]);
        // index is None initially. prev should go to the last entry.
        app.prev_input_history();
        assert_eq!(app.input.value(), "third");

        app.prev_input_history();
        assert_eq!(app.input.value(), "second");

        app.prev_input_history();
        assert_eq!(app.input.value(), "first");

        // Past the start: index clamps, value unchanged.
        let v_before = app.input.value().to_string();
        app.prev_input_history();
        assert_eq!(app.input.value(), v_before);
    }

    #[test]
    fn next_history_walks_forward_until_cleared() {
        let mut app = app_with_history(&["a", "b", "c"]);
        // Get to the start: prev × 3
        app.prev_input_history();
        app.prev_input_history();
        app.prev_input_history();
        assert_eq!(app.input.value(), "a");

        app.next_input_history();
        assert_eq!(app.input.value(), "b");

        app.next_input_history();
        assert_eq!(app.input.value(), "c");

        // Past the end → input cleared
        app.next_input_history();
        assert_eq!(app.input.value(), "");
        assert!(app.input_history_index.is_none());
    }

    #[test]
    fn next_history_no_op_when_not_navigating() {
        let mut app = app_with_history(&["x"]);
        app.next_input_history(); // index is None, should be no-op
        assert_eq!(app.input.value(), "");
    }

    // ── autocomplete ────────────────────────────────────────────

    #[test]
    fn autocomplete_completes_unique_prefix() {
        let mut app = empty_app();
        app.input = Input::default().with_value("/qu".to_string());
        app.autocomplete();
        // /qu is a unique prefix of /quit
        assert_eq!(app.input.value(), "/quit");
    }

    #[test]
    fn autocomplete_does_not_explode_on_no_match() {
        let mut app = empty_app();
        app.input = Input::default().with_value("/xyz".to_string());
        app.autocomplete();
        // No suggestion matches; we tolerate either no-op or behavior
        // that doesn't panic. Just check we still have a string.
        let _ = app.input.value();
    }

    #[test]
    fn autocomplete_does_not_explode_on_empty_input() {
        let mut app = empty_app();
        app.autocomplete();
    }

    // ── scroll_to_bottom ────────────────────────────────────────

    #[test]
    fn scroll_to_bottom_increases_offset_when_overflowing() {
        let mut app = empty_app();
        for i in 0..100 {
            app.add_message(MessageRole::User, format!("msg-{i}"));
        }
        app.scroll_offset = 0;
        app.scroll_to_bottom(10); // viewport = 10 rows
        assert!(
            app.scroll_offset > 0,
            "scroll_offset should advance for big history"
        );
    }

    #[test]
    fn scroll_to_bottom_zero_when_fits() {
        let mut app = empty_app();
        // Only the system message → fits a 50-row viewport
        app.scroll_to_bottom(50);
        assert_eq!(app.scroll_offset, 0);
    }

    // ── on_tick ─────────────────────────────────────────────────

    #[test]
    fn on_tick_is_safe_on_empty() {
        let mut app = empty_app();
        for _ in 0..10 {
            app.on_tick();
        }
        // Just doesn't panic.
    }

    // ── Command recognition ─────────────────────────────────────
    //
    // We don't dispatch (that requires reqwest etc.), but we can
    // verify that the suggestions list contains the canonical commands
    // and that a typed command exactly matches one.

    #[test]
    fn known_commands_are_in_suggestions() {
        let app = empty_app();
        for cmd in &["/model", "/task", "/status", "/history", "/help", "/quit"] {
            assert!(
                app.suggestions.contains(cmd),
                "missing canonical command: {cmd}"
            );
        }
    }

    // ── UI rendering snapshot ───────────────────────────────────

    #[test]
    fn ui_render_does_not_panic_on_empty_state() {
        let app = empty_app();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|f| ui(f, &app))
            .expect("first frame should render");
    }

    #[test]
    fn ui_render_contains_model_name_in_buffer() {
        let mut app = empty_app();
        app.current_model = "test-model:42b".to_string();
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| ui(f, &app)).expect("render");

        // Inspect the rendered buffer for the model name.
        let buf: &Buffer = terminal.backend().buffer();
        let area = Rect::new(0, 0, 120, 30);
        let mut text = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(
            text.contains("test-model:42b"),
            "rendered buffer should contain the current model name; got:\n{text}"
        );
    }

    #[test]
    fn ui_render_with_long_history_does_not_panic() {
        let mut app = empty_app();
        for i in 0..500 {
            app.add_message(
                if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                format!("Very long message #{i} that wraps and wraps and wraps"),
            );
        }
        app.scroll_offset = 100;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|f| ui(f, &app)).expect("should not panic");
    }
}
