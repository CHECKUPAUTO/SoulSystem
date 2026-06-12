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

/// A single hop in a redirect chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RedirectHop {
    pub url: String,
    pub status: u16,
}

/// Trace redirect chain for a given URL up to `max_hops`.
pub async fn trace_redirects(url: &str, max_hops: usize) -> Vec<RedirectHop> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("AVID-Scout/1.0")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut hops = Vec::new();
    let mut current = url.to_string();
    for _ in 0..max_hops {
        let Ok(res) = client.head(&current).send().await else {
            break;
        };
        let status = res.status().as_u16();
        hops.push(RedirectHop {
            url: current.clone(),
            status,
        });
        if ![301u16, 302, 303, 307, 308].contains(&status) {
            break;
        }
        if let Some(loc) = res.headers().get("location").and_then(|h| h.to_str().ok()) {
            current = if loc.starts_with("http") {
                loc.to_string()
            } else {
                url::Url::parse(&current)
                    .ok()
                    .and_then(|b| b.join(loc).ok())
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| loc.to_string())
            };
        } else {
            break;
        }
    }
    hops
}
