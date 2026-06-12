use serde_json::Value;

/// Extract JSON-LD script blocks from HTML and return parsed values.
#[must_use]
pub fn extract_json_ld(html: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let parts: Vec<&str> = html.split("script").collect();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let lower = part.to_lowercase();
        if lower.contains("application/ld+json") {
            if let Some(start) = part.find('>') {
                let after = &part[start + 1..];
                if let Some(end) = after.find('<') {
                    let json_text = &after[..end];
                    if let Ok(val) = serde_json::from_str::<Value>(json_text.trim()) {
                        results.push(val);
                    }
                }
            }
        }
    }
    results
}

/// Extract the first `@graph` or top-level `@type` name from JSON-LD.
#[must_use]
pub fn extract_schema_type(json_ld: &Value) -> Option<String> {
    if let Some(graph) = json_ld.get("@graph") {
        if let Some(arr) = graph.as_array() {
            return arr
                .first()
                .and_then(|v| v.get("@type"))
                .and_then(|t| t.as_str())
                .map(String::from);
        }
    }
    json_ld
        .get("@type")
        .and_then(|t| t.as_str())
        .map(String::from)
}
