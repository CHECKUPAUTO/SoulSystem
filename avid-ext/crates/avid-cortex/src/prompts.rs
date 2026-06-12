use handlebars::Handlebars;
use serde::Serialize;

/// System for managing and rendering prompt templates.
pub struct PromptSystem<'a> {
    hb: Handlebars<'a>,
}

impl<'a> PromptSystem<'a> {
    pub fn new() -> Self {
        let mut hb = Handlebars::new();
        hb.set_strict_mode(true);
        Self { hb }
    }

    pub fn register_template(&mut self, name: &str, template: &str) -> Result<(), anyhow::Error> {
        self.hb.register_template_string(name, template)?;
        Ok(())
    }

    pub fn render<T: Serialize>(&self, name: &str, data: &T) -> Result<String, anyhow::Error> {
        let rendered = self.hb.render(name, data)?;
        Ok(rendered)
    }

    pub fn count_tokens(text: &str) -> usize {
        text.len() / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_prompt_rendering() {
        let mut ps = PromptSystem::new();
        ps.register_template("t1", "Hello {{name}}!").unwrap();
        let res = ps.render("t1", &json!({"name": "World"})).unwrap();
        assert_eq!(res, "Hello World!");
    }

    #[test]
    fn test_token_counting() {
        assert_eq!(PromptSystem::count_tokens("12345678"), 2);
    }
}
