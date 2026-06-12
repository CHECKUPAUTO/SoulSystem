use url::Url;

/// Minimal robots.txt parser.
///
/// Supports `User-agent: *`, `Disallow:`, `Allow:`, and `Crawl-delay:` directives.
#[derive(Debug, Clone, Default)]
pub struct RobotsTxt {
    rules: Vec<(Rule, bool)>,
    /// Optional crawl delay in seconds for the wildcard user-agent.
    pub crawl_delay: Option<f64>,
}

#[derive(Debug, Clone)]
enum Rule {
    Prefix(String),
}

impl RobotsTxt {
    /// Parse a robots.txt body.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let mut in_wildcard = false;
        let mut rules = Vec::new();
        let mut crawl_delay = None;

        for line in source.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, val)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim();
            let val = val.trim();

            if key.eq_ignore_ascii_case("User-agent") {
                in_wildcard = val == "*";
                continue;
            }
            if !in_wildcard {
                continue;
            }
            if key.eq_ignore_ascii_case("Disallow") {
                rules.push((Rule::Prefix(val.to_string()), false));
            } else if key.eq_ignore_ascii_case("Allow") {
                rules.push((Rule::Prefix(val.to_string()), true));
            } else if key.eq_ignore_ascii_case("Crawl-delay") {
                if let Ok(seconds) = val.parse::<f64>() {
                    crawl_delay = Some(seconds);
                }
            }
        }

        Self { rules, crawl_delay }
    }

    /// Check whether `url` is allowed to be crawled.
    #[must_use]
    pub fn is_allowed(&self, url: &Url) -> bool {
        let path = url.path();
        let mut allowed = true;
        for (rule, is_allow) in &self.rules {
            match rule {
                Rule::Prefix(prefix) if path.starts_with(prefix) => {
                    allowed = *is_allow;
                }
                _ => {}
            }
        }
        allowed
    }
}
