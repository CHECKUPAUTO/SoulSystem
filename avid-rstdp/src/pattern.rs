use sha2::{Digest, Sha256};

pub type PatternHash = [u8; 32];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatternSignature {
    pub hash: String,
    pub canonical: String,
}

impl PatternSignature {
    #[must_use]
    pub fn from_task(task_text: &str, url: Option<&str>) -> Self {
        let canonical = canonicalize(task_text, url);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        Self { hash: hex::encode(digest), canonical }
    }
}

fn canonicalize(text: &str, url: Option<&str>) -> String {
    let mut s = text.to_lowercase();
    s = collapse_urls(&s);
    let s: String = s.chars().map(|c| if c.is_ascii_digit() { 'N' } else { c }).collect();
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(u) = url {
        let domain = u.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or("");
        format!("{s} @ {domain}")
    } else {
        s
    }
}

fn collapse_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == 'h' && chars.peek() == Some(&'t') {
            let rest: String = chars.clone().take(6).collect();
            if rest.starts_with("ttp") {
                out.push_str("<URL>");
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() { break; }
                    chars.next();
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_structure_same_hash() {
        let a = PatternSignature::from_task("clone the api at https://example.com/v1/users", None);
        let b = PatternSignature::from_task("clone the api at https://example.org/v2/posts", None);
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn different_structure_different_hash() {
        let a = PatternSignature::from_task("clone the api at https://x.com", None);
        let b = PatternSignature::from_task("scrape the website at https://x.com", None);
        assert_ne!(a.hash, b.hash);
    }
}
