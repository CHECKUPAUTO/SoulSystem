/// Decode bytes using the named charset, falling back to UTF-8 lossy.
#[must_use]
pub fn decode(bytes: &[u8], charset: &str) -> String {
    use encoding_rs::*;
    let encoding = Encoding::for_label(charset.as_bytes()).unwrap_or(UTF_8);
    let (cow, _had_errors, _used_replacement) = encoding.decode(bytes);
    cow.into_owned()
}
