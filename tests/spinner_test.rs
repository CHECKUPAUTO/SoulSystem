//! Tests d'intégration pour le spinner animé.

use soulsystem::spinner::Spinner;

#[test]
fn test_spinner_integration_cycles_through_all_frames() {
    let mut spinner = Spinner::new();
    let mut seen = std::collections::HashSet::new();

    for _ in 0..10 {
        let frame = spinner.next_frame().to_string();
        seen.insert(frame);
    }

    assert!(
        seen.len() >= 2,
        "Spinner should cycle through multiple frames"
    );
}

#[test]
fn test_spinner_integration_default_new() {
    let spinner = Spinner::new();
    let spinner2 = Spinner::default();
    let mut s1 = spinner;
    let mut s2 = spinner2;
    assert_eq!(s1.next_frame(), s2.next_frame());
}

#[test]
fn test_spinner_integration_thread_safe_clone() {
    let mut spinner = Spinner::new();
    let mut clone = spinner.clone();
    assert_eq!(spinner.next_frame(), clone.next_frame());
}

#[test]
fn test_spinner_integration_long_running() {
    let mut spinner = Spinner::new();

    for _ in 0..1000 {
        let _frame = spinner.next_frame();
    }

    spinner.reset();
    let first = spinner.next_frame();
    assert!(!first.is_empty());
}
