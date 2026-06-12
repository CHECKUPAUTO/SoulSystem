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

/// Subscription / newsletter detection.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionDetection {
    pub has_newsletter_form: bool,
    pub has_paywall: bool,
    pub has_login: bool,
    pub has_register: bool,
    pub has_subscription_cta: bool,
    pub email_field_count: usize,
}

/// Detect subscription and membership signals.
#[must_use]
pub fn detect_subscriptions(html: &str, forms: &[super::forms::FormInfo]) -> SubscriptionDetection {
    let lower = html.to_lowercase();
    let mut has_newsletter = false;
    let mut email_count = 0;
    let mut has_login = false;
    let mut has_register = false;
    for form in forms {
        if let Some(action) = &form.action {
            let a = action.to_lowercase();
            if a.contains("newsletter") || a.contains("subscribe") || a.contains("abonnement") {
                has_newsletter = true;
            }
            if a.contains("login") || a.contains("signin") || a.contains("connexion") {
                has_login = true;
            }
            if a.contains("register") || a.contains("signup") || a.contains("inscription") {
                has_register = true;
            }
        }
        for field in &form.fields {
            if field.field_type == "email" || field.name.to_lowercase().contains("email") {
                email_count += 1;
            }
        }
    }
    SubscriptionDetection {
        has_newsletter_form: has_newsletter
            || lower.contains("newsletter")
            || lower.contains("subscribe"),
        has_paywall: lower.contains("paywall")
            || lower.contains("premium")
            || lower.contains("subscription required"),
        has_login,
        has_register,
        has_subscription_cta: lower.contains("subscribe")
            || lower.contains("join now")
            || lower.contains("get started"),
        email_field_count: email_count,
    }
}
