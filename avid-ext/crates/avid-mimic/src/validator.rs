use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationRule {
    TextPresent(String),
    ElementVisible(String),
}

/// Validates browser state against expected rules.
pub struct ActionValidator;

impl ActionValidator {
    pub fn validate(dom: &str, rules: &[ValidationRule]) -> Vec<bool> {
        rules
            .iter()
            .map(|rule| match rule {
                ValidationRule::TextPresent(t) => dom.contains(t),
                ValidationRule::ElementVisible(s) => dom.contains(s),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator() {
        let dom = "<html><body>Hello</body></html>";
        let rules = vec![
            ValidationRule::TextPresent("Hello".to_string()),
            ValidationRule::TextPresent("Bye".to_string()),
        ];
        let results = ActionValidator::validate(dom, &rules);
        assert_eq!(results, vec![true, false]);
    }
}
