use crate::recorder::Action;

/// Logic for learning from action sequences.
pub struct ActionLearner;

impl ActionLearner {
    pub fn classify_intent(actions: &[Action]) -> String {
        if actions.is_empty() {
            return "unknown".to_string();
        }
        let clicks = actions
            .iter()
            .filter(|a| matches!(a, Action::Click { .. }))
            .count();
        if clicks > 0 && actions.iter().any(|a| matches!(a, Action::KeyPress { .. })) {
            "form".to_string()
        } else if clicks > 0 {
            "nav".to_string()
        } else {
            "obs".to_string()
        }
    }

    pub fn generalize_macros(examples: Vec<Vec<Action>>) -> Vec<Action> {
        if examples.is_empty() {
            return vec![];
        }
        examples[0].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_classify_intent() {
        let actions = vec![Action::Click {
            x: 0.0,
            y: 0.0,
            button: "l".to_string(),
            selector: None,
        }];
        assert_eq!(ActionLearner::classify_intent(&actions), "nav");
    }

    #[test]
    fn test_generalize_macros() {
        let examples = vec![vec![Action::KeyPress {
            key: "a".to_string(),
        }]];
        let gen = ActionLearner::generalize_macros(examples);
        assert_eq!(gen.len(), 1);
    }
}
