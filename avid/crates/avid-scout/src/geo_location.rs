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

/// Geo-location guess.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GeoLocation {
    pub country_code: Option<String>,
    pub country_name: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

/// Guess geo-location from domain TLD or text content.
#[must_use]
pub fn guess_geo_location(host: &str, text: &str) -> GeoLocation {
    let mut geo = GeoLocation::default();
    if let Some(tld) = host.rsplit('.').next() {
        match tld.to_lowercase().as_str() {
            "fr" => {
                geo.country_code = Some("FR".to_string());
                geo.country_name = Some("France".to_string());
            }
            "de" => {
                geo.country_code = Some("DE".to_string());
                geo.country_name = Some("Germany".to_string());
            }
            "uk" | "co.uk" => {
                geo.country_code = Some("GB".to_string());
                geo.country_name = Some("United Kingdom".to_string());
            }
            "us" => {
                geo.country_code = Some("US".to_string());
                geo.country_name = Some("United States".to_string());
            }
            "jp" => {
                geo.country_code = Some("JP".to_string());
                geo.country_name = Some("Japan".to_string());
            }
            "cn" => {
                geo.country_code = Some("CN".to_string());
                geo.country_name = Some("China".to_string());
            }
            "ca" => {
                geo.country_code = Some("CA".to_string());
                geo.country_name = Some("Canada".to_string());
            }
            "es" => {
                geo.country_code = Some("ES".to_string());
                geo.country_name = Some("Spain".to_string());
            }
            "it" => {
                geo.country_code = Some("IT".to_string());
                geo.country_name = Some("Italy".to_string());
            }
            _ => {}
        }
    }
    if text.to_lowercase().contains("paris") && geo.country_code.is_none() {
        geo.country_code = Some("FR".to_string());
        geo.city = Some("Paris".to_string());
    }
    if text.to_lowercase().contains("berlin") && geo.country_code.is_none() {
        geo.country_code = Some("DE".to_string());
        geo.city = Some("Berlin".to_string());
    }
    geo
}
