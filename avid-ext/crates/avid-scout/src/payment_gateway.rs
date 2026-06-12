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

/// Detected payment gateways.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PaymentGateways {
    pub stripe: bool,
    pub paypal: bool,
    pub square: bool,
    pub adyen: bool,
    pub braintree: bool,
    pub mollie: bool,
    pub klarna: bool,
    pub apple_pay: bool,
    pub google_pay: bool,
    pub detected_count: usize,
}

const GATEWAYS: &[(&str, fn(&mut PaymentGateways))] = &[
    ("stripe.com", |g| g.stripe = true),
    ("stripe.js", |g| g.stripe = true),
    ("paypal.com", |g| g.paypal = true),
    ("paypalobjects.com", |g| g.paypal = true),
    ("squareup.com", |g| g.square = true),
    ("adyen.com", |g| g.adyen = true),
    ("braintree-api.com", |g| g.braintree = true),
    ("braintreegateway.com", |g| g.braintree = true),
    ("mollie.com", |g| g.mollie = true),
    ("klarna.com", |g| g.klarna = true),
    ("apple-pay", |g| g.apple_pay = true),
    ("google-pay", |g| g.google_pay = true),
    ("pay.google.com", |g| g.google_pay = true),
];

/// Detect payment gateways from HTML and links.
#[must_use]
pub fn detect_payment_gateways(html: &str, links: &[impl AsRef<str>]) -> PaymentGateways {
    let mut gates = PaymentGateways::default();
    let lower = html.to_lowercase();
    for (sig, setter) in GATEWAYS {
        if lower.contains(sig)
            || links
                .iter()
                .any(|l| l.as_ref().to_lowercase().contains(sig))
        {
            setter(&mut gates);
        }
    }
    gates.detected_count = [
        gates.stripe,
        gates.paypal,
        gates.square,
        gates.adyen,
        gates.braintree,
        gates.mollie,
        gates.klarna,
        gates.apple_pay,
        gates.google_pay,
    ]
    .iter()
    .filter(|&b| *b)
    .count();
    gates
}
