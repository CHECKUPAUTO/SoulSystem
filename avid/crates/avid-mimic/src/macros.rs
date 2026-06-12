use crate::recorder::Action;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro { pub name: String, pub actions: Vec<Action> }

/// System for managing reusable action macros.
pub struct MacroSystem { pub macros: HashMap<String, Macro> }

impl MacroSystem {
    pub fn new() -> Self { Self { macros: HashMap::new() } }
    pub fn register(&mut self, m: Macro) { self.macros.insert(m.name.clone(), m); }

    pub fn compose(&self, names: Vec<String>) -> Vec<Action> {
        let mut actions = Vec::new();
        for name in names {
            if let Some(m) = self.macros.get(&name) {
                actions.extend(m.actions.clone());
            }
        }
        actions
    }

    pub fn delete_macro(&mut self, name: &str) {
        self.macros.remove(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_macro_reg() {
        let mut sys = MacroSystem::new();
        sys.register(Macro { name: "t".to_string(), actions: vec![] });
        assert!(sys.macros.contains_key("t"));
    }

    #[test]
    fn test_macro_compose() {
        let mut sys = MacroSystem::new();
        sys.register(Macro { name: "a".to_string(), actions: vec![Action::MouseMove { x: 0.0, y: 0.0 }] });
        sys.register(Macro { name: "b".to_string(), actions: vec![Action::MouseMove { x: 1.0, y: 1.0 }] });
        let composed = sys.compose(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(composed.len(), 2);
    }

    #[test]
    fn test_macro_delete() {
        let mut sys = MacroSystem::new();
        sys.register(Macro { name: "del".to_string(), actions: vec![] });
        sys.delete_macro("del");
        assert!(!sys.macros.contains_key("del"));
    }
}
