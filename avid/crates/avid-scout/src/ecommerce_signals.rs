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

/// E-commerce signals.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EcommerceSignals {
    pub has_cart: bool,
    pub has_checkout: bool,
    pub has_wishlist: bool,
    pub has_reviews: bool,
    pub has_product_schema: bool,
    pub has_price: bool,
    pub has_add_to_cart: bool,
    pub estimated_products: usize,
}

/// Detect e-commerce signals.
#[must_use]
pub fn detect_ecommerce(html: &str, structured: &[serde_json::Value]) -> EcommerceSignals {
    let lower = html.to_lowercase();
    let has_product = structured.iter().any(|v| {
        v.get("@type").and_then(|t| t.as_str()).map_or(false, |t| {
            t.eq_ignore_ascii_case("Product") || t.eq_ignore_ascii_case("Offer")
        })
    });
    let product_count = lower.matches("product").count();
    EcommerceSignals {
        has_cart: lower.contains("cart") || lower.contains("basket") || lower.contains("panier"),
        has_checkout: lower.contains("checkout")
            || lower.contains("commander")
            || lower.contains("paiement"),
        has_wishlist: lower.contains("wishlist") || lower.contains("favoris"),
        has_reviews: lower.contains("review") || lower.contains("avis"),
        has_product_schema: has_product,
        has_price: lower.contains("price")
            || lower.contains("prix")
            || lower.contains("eur")
            || lower.contains("usd"),
        has_add_to_cart: lower.contains("add to cart") || lower.contains("ajouter au panier"),
        estimated_products: product_count,
    }
}
