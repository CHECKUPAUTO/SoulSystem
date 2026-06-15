#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz the code signing verification to ensure it never panics on
// arbitrary signature/key/code combinations.
fuzz_target!(|data: &[u8]| {
    use soulsystem::code_signing::{AuthorizedKeys, SignedCode, verify_code};

    if data.len() < 64 {
        return;
    }

    let code_len = std::cmp::min(data.len() / 3, 1024);
    let code = String::from_utf8_lossy(&data[..code_len]).to_string();
    let sig_start = code_len;
    let sig_end = std::cmp::min(sig_start + 64, data.len());
    let key_start = sig_end;
    let key_end = std::cmp::min(key_start + 32, data.len());

    let signature = data[sig_start..sig_end].to_vec();
    let public_key = data[key_start..key_end].to_vec();

    let signed = SignedCode {
        code,
        signature,
        public_key: public_key.clone(),
    };

    let mut auth = AuthorizedKeys::new();
    auth.keys.insert(public_key);

    // verify_code must never panic
    let _ = verify_code(&signed, &auth);
});
