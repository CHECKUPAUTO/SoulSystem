use soulsystem::audit_log::AuditLog;

#[test]
fn test_audit_log_basic() {
    let mut log = AuditLog::new_test().unwrap();

    let entry = log.log("Clawd", "respond", "Answered query").unwrap();
    assert_eq!(entry.module, "Clawd");
    assert_eq!(entry.action, "respond");
    assert!(!entry.hash.is_empty());
    assert!(!entry.previous_hash.is_empty());

    // Verify entries are retrievable
    let entries = log.get_all().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].module, "Clawd");
}

#[test]
fn test_multiple_entries() {
    let mut log = AuditLog::new_test().unwrap();
    log.log("A", "b", "c").unwrap();
    log.log("D", "e", "f").unwrap();
    log.log("G", "h", "i").unwrap();

    let entries = log.get_all().unwrap();
    assert_eq!(entries.len(), 3);

    // Each entry should have a non-empty hash
    for entry in &entries {
        assert!(!entry.hash.is_empty());
        assert!(!entry.previous_hash.is_empty());
    }
}
