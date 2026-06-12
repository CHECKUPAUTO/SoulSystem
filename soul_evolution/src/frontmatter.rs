use std::collections::HashMap;

const FM_DELIMITER: &str = "---";

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub fields: HashMap<String, FrontmatterValue>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub enum FrontmatterValue {
    String(String),
    StringArray(Vec<String>),
    Integer(i64),
    Float(f64),
    Bool(bool),
}

impl Frontmatter {
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.fields.get(key).and_then(|v| match v {
            FrontmatterValue::String(s) => Some(s.clone()),
            _ => None,
        })
    }

    pub fn get_string_array(&self, key: &str) -> Option<Vec<String>> {
        self.fields.get(key).and_then(|v| match v {
            FrontmatterValue::StringArray(a) => Some(a.clone()),
            FrontmatterValue::String(s) => {
                Some(s.split(',').map(|s| s.trim().to_string()).collect())
            }
            _ => None,
        })
    }

    pub fn get_integer(&self, key: &str) -> Option<i64> {
        self.fields.get(key).and_then(|v| match v {
            FrontmatterValue::Integer(i) => Some(*i),
            FrontmatterValue::String(s) => s.parse().ok(),
            _ => None,
        })
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }
}

pub fn parse_frontmatter(content: &str) -> Option<(Frontmatter, usize)> {
    let content = content.trim_start();

    if !content.starts_with(FM_DELIMITER) {
        return None;
    }

    let content_after_first = &content[FM_DELIMITER.len()..];
    let end_pos = content_after_first.find(FM_DELIMITER)?;
    let yaml_str = content_after_first[..end_pos].trim();
    let total_consumed = FM_DELIMITER.len() + end_pos + FM_DELIMITER.len();

    let fields = parse_yaml_fields(yaml_str);
    let fm = Frontmatter {
        fields,
        raw: yaml_str.to_string(),
    };

    Some((fm, total_consumed))
}

fn parse_yaml_fields(yaml_str: &str) -> HashMap<String, FrontmatterValue> {
    let mut map = HashMap::new();

    for line in yaml_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = parse_key_value(trimmed) {
            map.insert(key.to_lowercase(), value);
        }
    }

    map
}

fn parse_key_value(line: &str) -> Option<(String, FrontmatterValue)> {
    let colon_pos = line.find(':')?;
    let key = line[..colon_pos].trim().to_string();

    if key.is_empty() {
        return None;
    }

    let value_str = line[colon_pos + 1..].trim();

    if value_str.is_empty() {
        return Some((key, FrontmatterValue::String(String::new())));
    }

    if value_str == "true" {
        return Some((key, FrontmatterValue::Bool(true)));
    }
    if value_str == "false" {
        return Some((key, FrontmatterValue::Bool(false)));
    }

    if value_str.starts_with('"') && value_str.ends_with('"') {
        let inner = &value_str[1..value_str.len() - 1];
        return Some((key, FrontmatterValue::String(inner.to_string())));
    }

    if value_str.starts_with('[') {
        let inner = value_str.trim_start_matches('[').trim_end_matches(']');
        let items: Vec<String> = inner
            .split(',')
            .map(|s| {
                let s = s.trim();
                if (s.starts_with('"') && s.ends_with('"'))
                    || (s.starts_with('\'') && s.ends_with('\''))
                {
                    s[1..s.len() - 1].to_string()
                } else {
                    s.to_string()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        return Some((key, FrontmatterValue::StringArray(items)));
    }

    if let Ok(i) = value_str.parse::<i64>() {
        return Some((key, FrontmatterValue::Integer(i)));
    }

    if let Ok(f) = value_str.parse::<f64>() {
        return Some((key, FrontmatterValue::Float(f)));
    }

    Some((key, FrontmatterValue::String(value_str.to_string())))
}

pub fn extract_body(content: &str) -> &str {
    let content = content.trim_start();
    if let Some(after_first) = content.strip_prefix(FM_DELIMITER) {
        if let Some(end_pos) = after_first.find(FM_DELIMITER) {
            let after_fm = &after_first[end_pos + FM_DELIMITER.len()..];
            return after_fm.trim();
        }
    }
    content.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_frontmatter() {
        let content = "---\nname: test-agent\ndescription: A test agent\nmodel: opus\n---\n\nBody content here";
        let (fm, consumed) = parse_frontmatter(content).unwrap();
        assert_eq!(fm.get_string("name").unwrap(), "test-agent");
        assert_eq!(fm.get_string("model").unwrap(), "opus");
        assert!(consumed > 0);
    }

    #[test]
    fn test_parse_array() {
        let content = "---\ntools: [\"Read\", \"Grep\", \"Bash\"]\n---";
        let (fm, _) = parse_frontmatter(content).unwrap();
        let tools = fm.get_string_array("tools").unwrap();
        assert_eq!(tools, vec!["Read", "Grep", "Bash"]);
    }

    #[test]
    fn test_no_frontmatter() {
        assert!(parse_frontmatter("Just content without frontmatter").is_none());
    }

    #[test]
    fn test_extract_body() {
        let content = "---\nname: test\n---\n\nReal body\nhere";
        assert_eq!(extract_body(content), "Real body\nhere");
    }

    #[test]
    fn test_multiline_array() {
        let content = "---\ntools:\n  - Read\n  - Grep\n---";
        let (fm, _) = parse_frontmatter(content).unwrap();
        assert!(fm.has_key("tools"));
    }

    #[test]
    fn test_integer_and_float() {
        let content = "---\nmaxTurns: 10\nscore: 3.5\n---";
        let (fm, _) = parse_frontmatter(content).unwrap();
        assert_eq!(fm.get_integer("maxturns").unwrap(), 10);
    }
}
