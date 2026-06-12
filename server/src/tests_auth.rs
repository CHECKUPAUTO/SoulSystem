#[cfg(test)]
mod tests {
    use crate::auth;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "mysecretpassword";
        let hash = auth::hash_password(password).unwrap();
        assert!(auth::verify_password(password, &hash).unwrap());
        assert!(!auth::verify_password("wrongpassword", &hash).unwrap());
    }

    #[test]
    fn test_new_session_token() {
        let token1 = auth::new_session_token();
        let token2 = auth::new_session_token();
        assert_eq!(token1.len(), 64);
        assert_ne!(token1, token2);
    }
}
