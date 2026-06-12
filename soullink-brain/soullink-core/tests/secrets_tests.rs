//! Integration tests for `secrets` loader — exercises the filesystem path
//! with real permissions under `/tmp`.

use std::io::Write;

use soullink_core::secrets::{self, InternalToken, SecretsError};

#[test]
fn token_hex_roundtrip() {
    let original = InternalToken([0xAB; 32]);
    let hex = original.to_hex();
    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    let round = InternalToken::from_hex(&hex).unwrap();
    assert_eq!(round.0, original.0);
}

#[test]
fn token_hex_rejects_bad_length() {
    assert!(matches!(
        InternalToken::from_hex(""),
        Err(SecretsError::InvalidToken)
    ));
    assert!(matches!(
        InternalToken::from_hex("abcd"),
        Err(SecretsError::InvalidToken)
    ));
    assert!(matches!(
        InternalToken::from_hex(&"a".repeat(63)),
        Err(SecretsError::InvalidToken)
    ));
    assert!(matches!(
        InternalToken::from_hex(&"a".repeat(65)),
        Err(SecretsError::InvalidToken)
    ));
}

#[test]
fn token_hex_rejects_non_hex() {
    assert!(matches!(
        InternalToken::from_hex(&"z".repeat(64)),
        Err(SecretsError::InvalidToken)
    ));
    assert!(matches!(
        InternalToken::from_hex(&"!".repeat(64)),
        Err(SecretsError::InvalidToken)
    ));
    // Mixed valid + invalid
    let mut s = "a".repeat(62);
    s.push('g');
    s.push('h');
    assert!(matches!(
        InternalToken::from_hex(&s),
        Err(SecretsError::InvalidToken)
    ));
}

#[test]
fn token_hex_accepts_both_cases() {
    let lower = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let upper = lower.to_uppercase();
    let a = InternalToken::from_hex(lower).unwrap();
    let b = InternalToken::from_hex(&upper).unwrap();
    assert_eq!(a.0, b.0);
}

#[test]
fn token_debug_redacts() {
    let t = InternalToken([0xDE; 32]);
    let s = format!("{:?}", t);
    assert!(!s.contains("DE"), "debug output leaks token bytes: {s}");
    assert!(s.contains("redacted"));
}

#[test]
fn load_from_missing_file() {
    let p = std::path::Path::new("/tmp/nonexistent-secrets-soullink-xyz-42");
    assert!(matches!(
        secrets::load_from(p),
        Err(SecretsError::NotFound(_))
    ));
}

#[cfg(unix)]
#[test]
fn load_from_insecure_permissions_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        tmp.as_file(),
        "SOULLINK_INTERNAL_TOKEN=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    )
    .unwrap();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o644)).unwrap();

    let err = secrets::load_from(tmp.path()).unwrap_err();
    assert!(
        matches!(err, SecretsError::InsecurePermissions(_)),
        "got {err:?}"
    );
}

#[cfg(unix)]
#[test]
fn load_from_secure_permissions_ok() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(
        tmp.as_file(),
        "SOULLINK_INTERNAL_TOKEN=abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    )
    .unwrap();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600)).unwrap();

    // Non-root owner → warn but not fatal
    let t = secrets::load_from(tmp.path()).unwrap();
    assert_eq!(t.0[0], 0xAB);
    assert_eq!(t.0[31], 0x89);
}

#[test]
fn load_from_missing_key() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(tmp.as_file(), "SOMETHING_ELSE=foo").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let err = secrets::load_from(tmp.path()).unwrap_err();
    assert!(matches!(err, SecretsError::MissingKey(_)));
}

#[test]
fn load_from_handles_comments_and_quotes() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    writeln!(tmp.as_file(), "# this is a comment").unwrap();
    writeln!(tmp.as_file(), "").unwrap();
    writeln!(tmp.as_file(),
             "SOULLINK_INTERNAL_TOKEN=\"abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789\"").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(secrets::load_from(tmp.path()).is_ok());
}
