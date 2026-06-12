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

/// Discovered API endpoints.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ApiDiscovery {
    pub has_openapi: bool,
    pub has_swagger: bool,
    pub has_graphql: bool,
    pub has_rest: bool,
    pub openapi_url: Option<String>,
    pub swagger_url: Option<String>,
    pub graphql_url: Option<String>,
}

/// Detect API patterns from HTML and links.
#[must_use]
pub fn discover_apis(html: &str, links: &[impl AsRef<str>]) -> ApiDiscovery {
    let lower = html.to_lowercase();
    let mut apis = ApiDiscovery::default();
    for link in links {
        let l = link.as_ref().to_lowercase();
        if l.contains("/openapi.json") || l.contains("/openapi.yaml") {
            apis.has_openapi = true;
            apis.openapi_url = Some(link.as_ref().to_string());
        }
        if l.contains("/swagger-ui") || l.contains("/swagger.json") {
            apis.has_swagger = true;
            apis.swagger_url = Some(link.as_ref().to_string());
        }
        if l.contains("/graphql") || l.contains("/gql") {
            apis.has_graphql = true;
            apis.graphql_url = Some(link.as_ref().to_string());
        }
        if l.contains("/api/") || l.contains("/v1/") || l.contains("/v2/") {
            apis.has_rest = true;
        }
    }
    if lower.contains("openapi") && !apis.has_openapi {
        apis.has_openapi = true;
    }
    if lower.contains("swagger") && !apis.has_swagger {
        apis.has_swagger = true;
    }
    if lower.contains("graphql") && !apis.has_graphql {
        apis.has_graphql = true;
    }
    apis
}
