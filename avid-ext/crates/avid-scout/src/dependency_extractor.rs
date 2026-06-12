use regex::Regex;

/// Extracts dependency information (e.g., from package.json or requirements.txt content shown on page).
pub fn extract_dependencies(text: &str) -> DependencyReport {
    let mut report = DependencyReport::default();

    // Look for common patterns like "react": "^18.0.0"
    let dep_regex = Regex::new(r#""([^"]+)":\s*"([\^~]?\d+\.\d+\.\d+)""#).unwrap();

    for cap in dep_regex.captures_iter(text) {
        report.dependencies.push(format!("{}@{}", &cap[1], &cap[2]));
    }

    report
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DependencyReport {
    pub dependencies: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dependencies() {
        let text = r#"{"dependencies": {"react": "^18.2.0", "lodash": "4.17.21"}}"#;
        let report = extract_dependencies(text);
        assert_eq!(report.dependencies.len(), 2);
        assert!(report.dependencies.contains(&"react@^18.2.0".to_string()));
    }
}
