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

/// CMS detection result.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CmsDetection {
    pub detected_cms: Option<String>,
    pub is_wordpress: bool,
    pub is_shopify: bool,
    pub is_drupal: bool,
    pub is_joomla: bool,
    pub is_magento: bool,
    pub is_wix: bool,
    pub is_squarespace: bool,
    pub is_webflow: bool,
    pub is_nextjs: bool,
    pub is_nuxtjs: bool,
    pub confidence: u8,
}

const CMS_SIGNATURES: &[(&str, fn(&mut CmsDetection))] = &[
    ("wp-content", |c| c.is_wordpress = true),
    ("wp-includes", |c| c.is_wordpress = true),
    ("/wp-json/", |c| c.is_wordpress = true),
    ("shopify.com", |c| c.is_shopify = true),
    ("myshopify.com", |c| c.is_shopify = true),
    ("cdn.shopify.com", |c| c.is_shopify = true),
    ("drupal", |c| c.is_drupal = true),
    ("/sites/default/", |c| c.is_drupal = true),
    ("joomla", |c| c.is_joomla = true),
    ("/media/jui/", |c| c.is_joomla = true),
    ("magento", |c| c.is_magento = true),
    ("mage-", |c| c.is_magento = true),
    ("wix.com", |c| c.is_wix = true),
    ("static.wixstatic.com", |c| c.is_wix = true),
    ("squarespace.com", |c| c.is_squarespace = true),
    ("static1.squarespace.com", |c| c.is_squarespace = true),
    ("webflow.com", |c| c.is_webflow = true),
    ("cdn.prod.website-files.com", |c| c.is_webflow = true),
    ("__NEXT_DATA__", |c| c.is_nextjs = true),
    ("next/dist", |c| c.is_nextjs = true),
    ("__NUXT__", |c| c.is_nuxtjs = true),
    ("_nuxt/", |c| c.is_nuxtjs = true),
];

/// Detect CMS from HTML, headers and links.
#[must_use]
pub fn detect_cms(html: &str, headers: &[(String, String)]) -> CmsDetection {
    let mut cms = CmsDetection::default();
    let lower = html.to_lowercase();
    let mut matches = 0;
    for (sig, setter) in CMS_SIGNATURES {
        if lower.contains(sig) {
            setter(&mut cms);
            matches += 1;
        }
    }
    for (key, value) in headers {
        let v = value.to_lowercase();
        if key.to_lowercase() == "x-powered-by" {
            if v.contains("wordpress") {
                cms.is_wordpress = true;
                matches += 1;
            }
            if v.contains("drupal") {
                cms.is_drupal = true;
                matches += 1;
            }
            if v.contains("shopify") {
                cms.is_shopify = true;
                matches += 1;
            }
        }
        if key.to_lowercase() == "x-shopify-stage" {
            cms.is_shopify = true;
            matches += 1;
        }
    }
    cms.detected_cms = if cms.is_wordpress {
        Some("WordPress".to_string())
    } else if cms.is_shopify {
        Some("Shopify".to_string())
    } else if cms.is_drupal {
        Some("Drupal".to_string())
    } else if cms.is_joomla {
        Some("Joomla".to_string())
    } else if cms.is_magento {
        Some("Magento".to_string())
    } else if cms.is_wix {
        Some("Wix".to_string())
    } else if cms.is_squarespace {
        Some("Squarespace".to_string())
    } else if cms.is_webflow {
        Some("Webflow".to_string())
    } else if cms.is_nextjs {
        Some("Next.js".to_string())
    } else if cms.is_nuxtjs {
        Some("Nuxt.js".to_string())
    } else {
        None
    };
    cms.confidence = (matches * 20).min(100) as u8;
    cms
}
