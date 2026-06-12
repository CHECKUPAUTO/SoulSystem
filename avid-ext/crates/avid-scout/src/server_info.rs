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

/// Server information.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    pub server: Option<String>,
    pub powered_by: Option<String>,
    pub via: Option<String>,
    pub cache_status: Option<String>,
    pub cdn_provider: Option<String>,
    pub has_http2: bool,
    pub has_http3: bool,
    pub has_brotli: bool,
    pub has_gzip: bool,
}

/// Extract server info from HTTP response headers.
#[must_use]
pub fn extract_server_info(headers: &[(String, String)]) -> ServerInfo {
    let mut info = ServerInfo::default();
    for (key, value) in headers {
        let k = key.to_lowercase();
        match k.as_str() {
            "server" => info.server = Some(value.clone()),
            "x-powered-by" => info.powered_by = Some(value.clone()),
            "via" => info.via = Some(value.clone()),
            "x-cache" | "x-cache-status" | "cf-cache-status" => {
                info.cache_status = Some(value.clone());
            }
            "alt-svc" => {
                let v = value.to_lowercase();
                info.has_http2 = v.contains("h2");
                info.has_http3 = v.contains("h3") || v.contains("quic");
            }
            "content-encoding" => {
                let v = value.to_lowercase();
                info.has_brotli = v.contains("br");
                info.has_gzip = v.contains("gzip");
            }
            _ => {}
        }
        if k.starts_with("x-cdn") || k.starts_with("cf-") || k == "x-fastly-request-id" {
            info.cdn_provider = Some(value.clone());
        }
    }
    info
}
