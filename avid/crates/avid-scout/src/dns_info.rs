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

use std::net::ToSocketAddrs;

/// DNS information.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DnsInfo {
    pub a_records: Vec<String>,
    pub has_mx: bool,
    pub txt_records: Vec<String>,
    pub resolved_ips: Vec<String>,
}

/// Lookup basic DNS info via system resolver.
pub fn lookup_dns(host: &str) -> DnsInfo {
    let mut info = DnsInfo::default();
    // Resolve A records via ToSocketAddrs
    let addr = format!("{}:80", host);
    if let Ok(addrs) = addr.to_socket_addrs() {
        for a in addrs {
            let ip = a.ip().to_string();
            if !info.resolved_ips.contains(&ip) {
                info.resolved_ips.push(ip.clone());
                info.a_records.push(ip);
            }
        }
    }
    info
}
