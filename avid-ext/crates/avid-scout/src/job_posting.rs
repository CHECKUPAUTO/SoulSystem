#![allow(
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::bool_to_int_with_if,
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::use_self,
    clippy::redundant_clone,
    clippy::wildcard_imports,
    clippy::option_if_let_else,
    clippy::manual_split_once,
    clippy::match_wildcard_for_single_variants,
    clippy::single_char_pattern,
    clippy::range_plus_one,
    clippy::unnecessary_map_or,
    clippy::manual_pattern_char_comparison,
    clippy::suboptimal_flops,
    clippy::needless_collect,
    clippy::inefficient_to_string
)]

use regex::Regex;
use std::sync::OnceLock;

/// Job posting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobPosting {
    pub title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub salary: Option<String>,
    pub employment_type: Option<String>,
    pub description_snippet: Option<String>,
}

fn job_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(full[- ]?time|part[- ]?time|contract|freelance|internship|remote|hybrid)")
            .unwrap()
    })
}

/// Extract job postings from structured data or text heuristics.
#[must_use]
pub fn extract_jobs(text: &str, structured: &[serde_json::Value]) -> Vec<JobPosting> {
    let mut jobs = Vec::new();
    for item in structured {
        if let Some(t) = item.get("@type").and_then(|v| v.as_str()) {
            if t.eq_ignore_ascii_case("JobPosting") {
                jobs.push(JobPosting {
                    title: item.get("title").and_then(|v| v.as_str()).map(String::from),
                    company: item
                        .get("hiringOrganization")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    location: item
                        .get("jobLocation")
                        .and_then(|v| v.get("address"))
                        .and_then(|v| v.get("addressLocality"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    salary: item
                        .get("baseSalary")
                        .and_then(|v| v.get("value"))
                        .and_then(|v| v.get("value"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    employment_type: item
                        .get("employmentType")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    description_snippet: item
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.chars().take(200).collect()),
                });
            }
        }
    }
    if jobs.is_empty() && job_regex().is_match(text) {
        // Heuristic: look for job title patterns
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.len() > 10
                && trimmed.len() < 120
                && (trimmed.to_lowercase().contains("engineer")
                    || trimmed.to_lowercase().contains("manager")
                    || trimmed.to_lowercase().contains("developer")
                    || trimmed.to_lowercase().contains("designer"))
                && job_regex().is_match(trimmed)
            {
                jobs.push(JobPosting {
                    title: Some(trimmed.to_string()),
                    company: None,
                    location: None,
                    salary: None,
                    employment_type: None,
                    description_snippet: None,
                });
            }
        }
    }
    jobs.into_iter().take(10).collect()
}
