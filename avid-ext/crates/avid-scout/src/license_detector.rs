/// Detects software licenses.
pub fn detect_license(text: &str) -> LicenseReport {
    let mut report = LicenseReport::default();
    let t = text.to_lowercase();

    if t.contains("mit license") {
        report.licenses.push("MIT".to_string());
    }
    if t.contains("apache license") || t.contains("apache-2.0") {
        report.licenses.push("Apache-2.0".to_string());
    }
    if t.contains("gnu gpl") || t.contains("gplv3") {
        report.licenses.push("GPL".to_string());
    }

    report.has_license = !report.licenses.is_empty();
    report
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LicenseReport {
    pub has_license: bool,
    pub licenses: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_license() {
        let text = "This software is licensed under the MIT License.";
        let report = detect_license(text);
        assert!(report.has_license);
        assert_eq!(report.licenses[0], "MIT");
    }
}
