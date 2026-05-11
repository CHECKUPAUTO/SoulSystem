use axum::http::HeaderMap;
use crate::corpus::Corpus;

pub fn authenticate(headers: &HeaderMap, corpus: &Corpus) -> Option<i64> {
    let key = headers.get("X-API-Key")?.to_str().ok()?;
    corpus.tenant_by_api_key(key).ok().flatten()
}
