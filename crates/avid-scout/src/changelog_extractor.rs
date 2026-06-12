use regex::Regex;

/// Extracts changelog entries from page text.
pub fn extract_changelog(text: &str) -> ChangelogReport {
    let mut report = ChangelogReport::default();

    let version_regex = Regex::new(r"(?i)v?\d+\.\d+\.\d+").unwrap();
    let date_regex = Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();

    for line in text.lines() {
        if version_regex.is_match(line) {
            report.versions_found.push(line.trim().to_string());
        }
        if date_regex.is_match(line) {
            // simplified
        }
    }

    report.has_changelog = !report.versions_found.is_empty();
    report
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ChangelogReport {
    pub has_changelog: bool,
    pub versions_found: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_changelog() {
        let text = "Release v1.2.3 (2024-05-01)\n- Added cool feature\nRelease v1.2.2";
        let report = extract_changelog(text);
        assert!(report.has_changelog);
        assert!(report.versions_found.iter().any(|v| v.contains("1.2.3")));
    }
}
