#![allow(clippy::result_large_err)]
#![forbid(unsafe_code)]
#![deny(warnings)]
#![warn(clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::redundant_clone,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::doc_markdown,
    clippy::manual_split_once,
    clippy::match_wildcard_for_single_variants,
    clippy::single_match,
    clippy::cast_possible_truncation,
    clippy::struct_excessive_bools,
    clippy::redundant_closure_for_method_calls,
    clippy::cast_lossless,
    clippy::manual_clamp,
    clippy::manual_midpoint,
    clippy::get_first,
    clippy::single_match_else,
    clippy::field_reassign_with_default,
    clippy::manual_let_else,
    clippy::new_without_default,
    clippy::collapsible_else_if,
    clippy::manual_contains,
    clippy::redundant_field_names,
    clippy::default_trait_access,
    clippy::let_and_return,
    clippy::type_complexity,
    clippy::used_underscore_binding,
    clippy::unnecessary_map_or,
    clippy::cognitive_complexity,
    unused_mut
)]

//! Web exploration engine for the AVID autonomous system.
//!
//! `ScoutEngine` crawls web pages, extracts links and content,
//! and navigates autonomously through documentation sites.

use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::Mutex;

pub mod accessibility;
pub mod ada_compliance;
pub mod adaptive_rate;
pub mod analytics;
pub mod api_discovery;
pub mod api_topology;
pub mod archive_checker;
pub mod auth;
pub mod breadcrumb;
pub mod broken_links;
pub mod cache;
pub mod carbon_footprint;
pub mod changelog_extractor;
pub mod charset;
pub mod classifier;
pub mod client;
pub mod cms_detection;
pub mod contacts;
pub mod content_quality;
pub mod cookie_inventory;
pub mod cookies_consent;
pub mod core_web_vitals;
pub mod dark_patterns;
pub mod dependency_extractor;
pub mod diff;
pub mod distributed;
pub mod dns_info;
pub mod ecommerce_signals;
pub mod email_verification;
pub mod entities;
pub mod event_extraction;
pub mod extractor;
pub mod faq_extraction;
pub mod feeds;
pub mod forms;
pub mod freshness;
pub mod geo_location;
pub mod graphql_detector;
pub mod heading_outline;
pub mod headless;
pub mod hreflang_audit;
pub mod i18n;
pub mod image_alt_audit;
pub mod images;
pub mod infinite_scroll;
pub mod internal_search;
pub mod international_phone;
pub mod job_posting;
pub mod keywords;
pub mod language;
pub mod lazy_loading;
pub mod license_detector;
pub mod link_popularity;
pub mod manifest_json;
pub mod metadata;
pub mod metrics;
pub mod ml_classifier;
pub mod mobile_friendly;
pub mod open_graph_video;
pub mod openapi_detector;
pub mod page_speed_insights;
pub mod pagination_detection;
pub mod payment_gateway;
pub mod pdf_extract;
pub mod performance_budget;
pub mod pricing;
pub mod privacy_policy;
pub mod proxy;
pub mod readability;
pub mod recipe_extraction;
pub mod redirect;
pub mod redirect_chain;
pub mod retry;
pub mod reviews;
pub mod robots;
pub mod rss_atom;
pub mod schema_validation;
pub mod screenshot;
pub mod search_suggestions;
pub mod security;
pub mod sentiment;
pub mod server_info;
pub mod service_worker;
pub mod sitemap;
pub mod sitemap_index;
pub mod social_meta;
pub mod source_links;
pub mod spam_score;
pub mod speed;
pub mod ssl_info;
pub mod stealth;
pub mod structured_data;
pub mod subscription_detection;
pub mod tables;
pub mod task;
pub mod tech_detect;
pub mod terms_detection;
pub mod video_extraction;
pub mod voice_search;
pub mod warc;
pub mod whois;

use ada_compliance::audit_ada_compliance;
use adaptive_rate::AdaptiveRateLimiter;
use analytics::detect_analytics;
use api_discovery::discover_apis;
use api_topology::{ApiTopology, ApiTopologyScanner};
use classifier::{classify_page, PageCategory};
use client::{ClientError, HttpClient};
use cms_detection::detect_cms;
use contacts::extract_contacts;
use content_quality::assess_quality;
use cookie_inventory::inventory_cookies;
use cookies_consent::detect_cookie_consent;
use dark_patterns::detect_dark_patterns;
use dns_info::lookup_dns;
use ecommerce_signals::detect_ecommerce;
use email_verification::verify_emails;
use entities::extract_entities;
use event_extraction::extract_events;
use extractor::{extract_links_from_dom, extract_text_from_dom, parse_html};
use faq_extraction::extract_faq;
use freshness::detect_freshness;
use geo_location::guess_geo_location;
use hreflang_audit::audit_hreflang;
use i18n::extract_i18n;
use infinite_scroll::detect_infinite_scroll;
use international_phone::extract_international_phones;
use job_posting::extract_jobs;
use keywords::compute_keyword_density;
use lazy_loading::detect_lazy_loading;
use metadata::{extract_metadata_from_dom, PageMetadata};
use metrics::ScoutMetrics;
use ml_classifier::{tokenise, TfidfClassifier};
use mobile_friendly::audit_mobile_friendly;
use page_speed_insights::simulate_psi;
use pagination_detection::detect_pagination;
use payment_gateway::detect_payment_gateways;
use performance_budget::compute_budget;
use pricing::extract_prices;
use privacy_policy::detect_privacy;
use readability::compute_readability;
use recipe_extraction::extract_recipes;
use retry::retry_with_backoff;
use reviews::extract_reviews;
use robots::RobotsTxt;
use rss_atom::detect_feeds;
use screenshot::capture_screenshot;
use security::scan_security_headers;
use sentiment::analyse_sentiment;
use server_info::extract_server_info;
use service_worker::detect_service_worker;
use sitemap::SitemapUrl;
use sitemap_index::parse_sitemap_index;
use social_meta::extract_social_meta;
use spam_score::compute_spam_score;
use speed::measure_speed;
use ssl_info::fetch_ssl_info;
use structured_data::{extract_json_ld, extract_schema_type};
use tech_detect::detect_technologies;
use terms_detection::detect_terms;
use video_extraction::extract_videos;
use whois::lookup_whois;

use changelog_extractor::extract_changelog;
use dependency_extractor::extract_dependencies;
use graphql_detector::detect_graphql;
use license_detector::detect_license;
use manifest_json::parse_manifest;
use open_graph_video::extract_og_video;
use openapi_detector::detect_openapi;
use url::Url;

/// A crawled page with metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScoutPage {
    pub url: String,
    pub title: Option<String>,
    pub links: Vec<String>,
    pub text: String,
    pub metadata: PageMetadata,
    pub status: u16,
    pub charset: Option<String>,
    pub structured_data: Vec<serde_json::Value>,
    pub images: Vec<images::ImageInfo>,
    pub schema_type: Option<String>,
    pub lang: Option<String>,
    pub tables: Vec<tables::Table>,
    pub forms: Vec<forms::FormInfo>,
    pub source_links: source_links::SourceLinks,
    pub tech_stack: tech_detect::TechStack,
    pub category: classifier::PageCategory,
    pub security: security::SecurityReport,
    pub contacts: contacts::Contacts,
    pub entities: entities::Entities,
    pub sentiment: sentiment::Sentiment,
    pub readability: readability::ReadabilityScore,
    pub keyword_density: keywords::KeywordDensity,
    pub accessibility: accessibility::AccessibilityReport,
    pub dark_patterns: dark_patterns::DarkPatterns,
    pub freshness: freshness::FreshnessSignals,
    pub speed: speed::SpeedMetrics,
    pub ssl: ssl_info::SslInfo,
    pub whois: whois::WhoisInfo,
    pub cookie_consent: cookies_consent::CookieConsent,
    pub prices: Vec<pricing::PriceInfo>,
    pub reviews: reviews::Reviews,
    pub breadcrumbs: Vec<breadcrumb::BreadcrumbItem>,
    pub screenshot: screenshot::ScreenshotInfo,
    pub ml_category: Option<String>,
    pub analytics: analytics::AnalyticsTags,
    pub i18n: i18n::I18nSignals,
    pub spam_score: spam_score::SpamScore,
    pub faq: Vec<faq_extraction::FaqItem>,
    pub social_meta: social_meta::SocialMeta,
    pub mobile_friendly: mobile_friendly::MobileFriendly,
    pub jobs: Vec<job_posting::JobPosting>,
    pub events: Vec<event_extraction::Event>,
    pub recipes: Vec<recipe_extraction::Recipe>,
    pub videos: Vec<video_extraction::Video>,
    pub dns: dns_info::DnsInfo,
    pub geo: geo_location::GeoLocation,
    pub content_quality: content_quality::ContentQuality,
    pub performance_budget: performance_budget::PerformanceBudget,
    pub api_discovery: api_discovery::ApiDiscovery,
    pub schema_validation: schema_validation::SchemaValidation,
    pub voice_search: voice_search::VoiceSearch,
    pub core_web_vitals: core_web_vitals::CoreWebVitals,
    pub carbon_footprint: carbon_footprint::CarbonFootprint,
    pub link_popularity: link_popularity::LinkPopularity,
    pub search_suggestions: search_suggestions::SearchSuggestions,
    pub ada_compliance: ada_compliance::AdaCompliance,
    pub cookie_inventory: cookie_inventory::CookieInventory,
    pub privacy_policy: privacy_policy::PrivacyPolicy,
    pub terms: terms_detection::TermsDetection,
    pub payment_gateways: payment_gateway::PaymentGateways,
    pub ecommerce_signals: ecommerce_signals::EcommerceSignals,
    pub cms: cms_detection::CmsDetection,
    pub server_info: server_info::ServerInfo,
    pub international_phones: Vec<international_phone::InternationalPhone>,
    pub verified_emails: Vec<email_verification::EmailVerification>,
    pub psi: page_speed_insights::PageSpeedInsights,
    pub hreflang_audit: hreflang_audit::HreflangAudit,
    pub image_alt_audit: image_alt_audit::ImageAltAudit,
    pub heading_outline: heading_outline::HeadingOutline,
    pub internal_search: internal_search::InternalSearch,
    pub subscription_detection: subscription_detection::SubscriptionDetection,
    pub sitemap_index: Vec<sitemap_index::SitemapIndexEntry>,
    pub feeds: Vec<rss_atom::FeedRef>,
    pub pagination: pagination_detection::Pagination,
    pub infinite_scroll: infinite_scroll::InfiniteScroll,
    pub lazy_loading: lazy_loading::LazyLoading,
    pub service_worker: service_worker::ServiceWorker,
    pub manifest: Option<manifest_json::ManifestSummary>,
    pub og_video: open_graph_video::OpenGraphVideo,
    pub openapi: openapi_detector::OpenApiReport,
    pub graphql: graphql_detector::GraphQlReport,
    pub topology: ApiTopology,
    pub changelog: changelog_extractor::ChangelogReport,
    pub license: license_detector::LicenseReport,
    pub dependencies: dependency_extractor::DependencyReport,
}

/// Configuration for the Scout engine.
#[derive(Debug, Clone)]
pub struct ScoutConfig {
    /// Maximum crawl depth (0 = unlimited).
    pub max_depth: u32,
    /// Minimum delay between consecutive requests to the same domain (ms).
    pub rate_limit_ms: u64,
    /// If set, only crawl URLs whose host is in this list.
    pub allowed_domains: Option<HashSet<String>>,
    /// If false, discard links that point to a different host.
    pub follow_external_links: bool,
    /// User-Agent string sent with every request.
    pub user_agent: String,
    /// If true, fetch and respect robots.txt.
    pub respect_robots_txt: bool,
    /// Preserve the raw HTML in `ScoutPage.raw_html` (cost: memory).
    pub keep_raw_html: bool,
    /// If true, render pages via headless Chromium so JS executes.
    /// Adds ~500ms–2s per page. Required for SPAs (TikTok, Instagram).
    /// Requires chromium/google-chrome in PATH.
    pub js_rendering: bool,
    /// JS render timeout in ms (default 10_000).
    pub js_render_timeout_ms: u64,
}

impl Default for ScoutConfig {
    fn default() -> Self {
        Self {
            max_depth: 2,
            rate_limit_ms: 500,
            allowed_domains: None,
            follow_external_links: false,
            user_agent: concat!("AVID-Scout/", env!("CARGO_PKG_VERSION")).to_string(),
            respect_robots_txt: true,
            keep_raw_html: false,
            js_rendering: false,
            js_render_timeout_ms: 10_000,
        }
    }
}

/// Errors that can occur during web exploration.
#[derive(Debug, thiserror::Error)]
pub enum ScoutError {
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
    #[error("queue error: {0}")]
    Queue(String),
    #[error("domain not allowed: {0}")]
    DomainNotAllowed(String),
    #[error("max depth reached at {0}")]
    MaxDepthReached(String),
    #[error("blocked by robots.txt: {0}")]
    BlockedByRobots(String),
}

/// Mutable inner state shared across cloned `ScoutEngine` handles.
#[derive(Debug)]
struct InnerState {
    seen_hashes: HashSet<String>,
    page_cache: HashMap<String, ScoutPage>,
    robots_cache: HashMap<String, RobotsTxt>,
    domain_tickers: HashMap<String, tokio::time::Instant>,
    metrics: ScoutMetrics,
    adaptive_rate: AdaptiveRateLimiter,
}

/// The Scout engine discovers and extracts web content.
#[derive(Debug)]
pub struct ScoutEngine {
    client: Arc<Mutex<HttpClient>>,
    config: Arc<ScoutConfig>,
    inner: Arc<Mutex<InnerState>>,
    headless: Option<Arc<crate::headless::HeadlessBrowser>>,
}

impl Clone for ScoutEngine {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            config: Arc::clone(&self.config),
            inner: Arc::clone(&self.inner),
            headless: self.headless.clone(),
        }
    }
}

impl ScoutEngine {
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(ScoutConfig::default())
    }

    #[must_use]
    pub fn with_config(config: ScoutConfig) -> Self {
        let inner = InnerState {
            seen_hashes: HashSet::new(),
            page_cache: HashMap::new(),
            robots_cache: HashMap::new(),
            domain_tickers: HashMap::new(),
            metrics: ScoutMetrics::new(),
            adaptive_rate: AdaptiveRateLimiter::new(500, 30_000),
        };
        Self {
            client: Arc::new(Mutex::new(HttpClient::new())),
            config: Arc::new(config),
            inner: Arc::new(Mutex::new(inner)),
            headless: None,
        }
    }

    pub async fn crawl(&self, url_str: &str, depth: u32) -> Result<Vec<ScoutPage>, ScoutError> {
        if depth > self.config.max_depth {
            return Err(ScoutError::MaxDepthReached(url_str.to_string()));
        }

        let url = Url::parse(url_str)?;
        let url_string = url.to_string();

        if let Some(ref allowed) = self.config.allowed_domains {
            let host = url.host_str().unwrap_or_default().to_lowercase();
            if !allowed.contains(&host) {
                return Err(ScoutError::DomainNotAllowed(host));
            }
        }

        let mut guard = self.inner.lock().await;

        if let Some(cached) = guard.page_cache.get(&url_string) {
            tracing::info!("cache hit: {}", url);
            guard.metrics.inc_pages_cached();
            return Ok(vec![cached.clone()]);
        }

        let hash = avid_anticlone::fingerprint_text(url.as_str());
        if guard.seen_hashes.contains(&hash) {
            tracing::info!("skipping duplicate URL: {}", url);
            return Ok(Vec::new());
        }
        guard.seen_hashes.insert(hash);
        guard.metrics.inc_pages_crawled();

        let host = url.host_str().unwrap_or_default().to_lowercase();

        if self.config.respect_robots_txt {
            let robots = guard.robots_cache.get(&host).cloned();
            drop(guard);
            let robots = if let Some(r) = robots {
                r
            } else {
                let robots_url = format!("{}://{host}/robots.txt", url.scheme());
                let robots_txt = self.fetch_raw(&robots_url).await.ok().unwrap_or_default();
                let parsed = RobotsTxt::parse(&robots_txt);
                let mut guard = self.inner.lock().await;
                guard.robots_cache.insert(host.clone(), parsed.clone());
                parsed
            };
            if !robots.is_allowed(&url) {
                let mut guard = self.inner.lock().await;
                guard.metrics.inc_robots_blocked();
                return Err(ScoutError::BlockedByRobots(url_string));
            }
            guard = self.inner.lock().await;
        }

        let now = tokio::time::Instant::now();
        let sleep_duration = if let Some(last) = guard.domain_tickers.get(&host) {
            let elapsed = now.duration_since(*last);
            let required = Duration::from_millis(self.config.rate_limit_ms);
            required.checked_sub(elapsed)
        } else {
            None
        };
        if sleep_duration.is_some() {
            guard.metrics.inc_domains_throttled();
        }
        guard.domain_tickers.insert(host, now);
        let adaptive_delay = guard.adaptive_rate.delay();
        drop(guard);
        if let Some(d) = sleep_duration {
            tokio::time::sleep(d).await;
        }
        if adaptive_delay > Duration::from_millis(0) {
            tokio::time::sleep(adaptive_delay).await;
        }

        let response = retry_with_backoff(|| async {
            let mut client = self.client.lock().await;
            client.fetch(&url).await
        })
        .await
        .map_err(|e| ScoutError::Network(e.to_string()))?;

        let is_html = response
            .content_type
            .as_ref()
            .is_none_or(|ct| ct.starts_with("text/html"));

        let headers_vec: Vec<(String, String)> = vec![];
        // Optimization: Parse HTML once and share the DOM across extraction modules.
        // This significantly reduces CPU overhead from repeated parsing of the same body.
        let (
            text,
            links,
            meta,
            structured,
            schema_type,
            imgs,
            lang,
            tbls,
            frms,
            src_links,
            tech,
            category,
            accessibility,
            breadcrumbs,
            image_alt_audit,
            heading_outline,
        ) = if is_html {
            let (
                text,
                raw_links,
                meta,
                imgs,
                tbls,
                frms,
                src_links,
                accessibility,
                breadcrumbs,
                lang,
                image_alt_audit,
                heading_outline,
            ) = {
                let dom = parse_html(&response.body);
                (
                    extract_text_from_dom(&dom),
                    extract_links_from_dom(&dom),
                    extract_metadata_from_dom(&dom),
                    images::extract_images_from_dom(&dom),
                    tables::extract_tables_from_dom(&dom),
                    forms::extract_forms_from_dom(&dom),
                    source_links::extract_source_links_from_dom(&dom),
                    accessibility::audit_accessibility_from_dom(&dom),
                    breadcrumb::extract_breadcrumbs_from_dom(&dom),
                    language::extract_language_from_dom(&dom),
                    image_alt_audit::audit_image_alt_from_dom(&dom),
                    heading_outline::extract_headings_from_dom(&dom),
                )
            };

            let structured = extract_json_ld(&response.body);
            let schema_type = structured.first().and_then(extract_schema_type);
            let links = self.resolve_links(&response.final_url, &raw_links);
            let tech = detect_technologies(&response.body, &headers_vec);
            let category = classify_page(url.as_str(), &meta, !frms.is_empty());
            (
                text,
                links,
                meta,
                structured,
                schema_type,
                imgs,
                lang,
                tbls,
                frms,
                src_links,
                tech,
                category,
                accessibility,
                breadcrumbs,
                image_alt_audit,
                heading_outline,
            )
        } else {
            (
                String::new(),
                Vec::new(),
                PageMetadata::default(),
                Vec::new(),
                None,
                Vec::new(),
                None,
                Vec::new(),
                Vec::new(),
                source_links::SourceLinks::default(),
                tech_detect::TechStack::default(),
                PageCategory::Unknown,
                accessibility::AccessibilityReport::default(),
                Vec::new(),
                image_alt_audit::ImageAltAudit::default(),
                heading_outline::HeadingOutline::default(),
            )
        };

        let contacts = extract_contacts(&text, &links);
        let entities = extract_entities(&text);
        let sentiment = analyse_sentiment(&text);
        let readability = compute_readability(&text);
        let keyword_density = compute_keyword_density(&text);
        let dark_patterns = detect_dark_patterns(&text, &frms);
        let freshness = detect_freshness(&text);
        let security = scan_security_headers(&headers_vec, url.scheme() == "https");
        let speed = measure_speed(Duration::from_secs(1), response.body.len());
        let ssl = fetch_ssl_info(url.host_str().unwrap_or(""), 443);
        let whois = lookup_whois(url.host_str().unwrap_or("")).unwrap_or_default();

        let cookie_consent = detect_cookie_consent(&response.body, &text);
        let prices = extract_prices(&text);
        let reviews = extract_reviews(&text);
        let screenshot = {
            match headless::HeadlessBrowser::launch().await {
                Ok(browser) => {
                    let browser = Arc::new(browser);
                    capture_screenshot(Some(&browser), url.as_str(), 1920, 1080, true).await
                }
                Err(e) => {
                    tracing::warn!("headless browser launch failed: {e}");
                    screenshot::ScreenshotInfo::unavailable(0, 0, false)
                }
            }
        };
        let ml_category = {
            let classifier = TfidfClassifier::new();
            let tokens = tokenise(&text);
            classifier.classify(&tokens).map(|(cat, _score)| cat)
        };

        let analytics = detect_analytics(&response.body);
        let i18n = extract_i18n(&response.body, lang.clone());
        let spam = compute_spam_score(&text, links.len(), imgs.len());
        let faq = extract_faq(&response.body, &text);
        let social_meta = extract_social_meta(&response.body);
        let mobile_friendly = audit_mobile_friendly(&response.body);

        let jobs = extract_jobs(&text, &structured);
        let events = extract_events(&structured);
        let recipes = extract_recipes(&structured);
        let videos = extract_videos(&response.body);
        let dns = lookup_dns(url.host_str().unwrap_or(""));
        let geo = guess_geo_location(url.host_str().unwrap_or(""), &text);
        let quality = assess_quality(&text, &response.body, imgs.len());
        let perf_budget = compute_budget(&response.body);

        let api_discovery = discover_apis(&response.body, &links);
        let schema_validation = schema_validation::validate_schema(&structured);
        let voice_search = voice_search::audit_voice_search(&text, &structured);
        let cwv = core_web_vitals::estimate_cwv(response.body.len(), frms.len(), imgs.len());
        let carbon = carbon_footprint::estimate_carbon(perf_budget.estimated_page_weight_kb);
        let link_pop = link_popularity::compute_link_popularity(&links, &text);
        let search_suggestions = search_suggestions::detect_search_suggestions(&structured);

        let ada = audit_ada_compliance(&response.body);
        let cookies = inventory_cookies(&response.body, &headers_vec);
        let privacy = detect_privacy(&response.body, &links);
        let terms = detect_terms(&response.body, &links);
        let payments = detect_payment_gateways(&response.body, &links);
        let ecommerce = detect_ecommerce(&response.body, &structured);
        let cms = detect_cms(&response.body, &headers_vec);
        let server_info = extract_server_info(&headers_vec);

        let international_phones = extract_international_phones(&text);
        let verified_emails = verify_emails(&text);
        let psi = simulate_psi(
            perf_budget.estimated_page_weight_kb,
            imgs.len(),
            frms.len(),
            mobile_friendly.has_viewport,
        );
        let hreflang_audit = audit_hreflang(&i18n.hreflangs, url.as_str());
        let internal_search = internal_search::detect_internal_search(&response.body, &frms);
        let subscription_detection =
            subscription_detection::detect_subscriptions(&response.body, &frms);

        let sitemap_index = parse_sitemap_index(&response.body);
        let feeds = detect_feeds(&response.body);
        let pagination = detect_pagination(&response.body, &links);
        let infinite_scroll = detect_infinite_scroll(&response.body);
        let lazy_loading = detect_lazy_loading(&response.body);
        let service_worker = detect_service_worker(&response.body);
        let manifest = service_worker
            .manifest_url
            .as_ref()
            .map(|_| parse_manifest("{}"));
        let og_video = extract_og_video(&response.body);
        let openapi = detect_openapi(&response.body, &links);
        let graphql = detect_graphql(&response.body, &links);
        let scanner = {
            let client_guard = self.client.lock().await;
            ApiTopologyScanner::new(client_guard.client.clone())
        };
        let topology = scanner.scan(url.as_str()).await.unwrap_or_default();
        let changelog = extract_changelog(&text);
        let license = detect_license(&text);
        let dependencies = extract_dependencies(&text);

        let page = ScoutPage {
            url: url_string.clone(),
            title: meta.title.clone(),
            links,
            text,
            metadata: meta,
            status: response.status,
            charset: response.charset,
            structured_data: structured,
            schema_type,
            images: imgs,
            lang,
            tables: tbls,
            forms: frms,
            source_links: src_links,
            tech_stack: tech,
            category,
            security,
            contacts,
            entities,
            sentiment,
            readability,
            keyword_density,
            accessibility,
            dark_patterns,
            freshness,
            speed,
            ssl,
            whois,
            cookie_consent,
            prices,
            reviews,
            breadcrumbs,
            screenshot,
            ml_category,
            analytics,
            i18n,
            spam_score: spam,
            faq,
            social_meta,
            mobile_friendly,
            jobs,
            events,
            recipes,
            videos,
            dns,
            geo,
            content_quality: quality,
            performance_budget: perf_budget,
            api_discovery,
            schema_validation,
            voice_search,
            core_web_vitals: cwv,
            carbon_footprint: carbon,
            link_popularity: link_pop,
            search_suggestions,
            ada_compliance: ada,
            cookie_inventory: cookies,
            privacy_policy: privacy,
            terms,
            payment_gateways: payments,
            ecommerce_signals: ecommerce,
            cms,
            server_info,
            international_phones,
            verified_emails,
            psi,
            hreflang_audit,
            image_alt_audit,
            heading_outline,
            internal_search,
            subscription_detection,
            sitemap_index,
            feeds,
            pagination,
            infinite_scroll,
            lazy_loading,
            service_worker,
            manifest,
            og_video,
            openapi,
            graphql,
            topology,
            changelog,
            license,
            dependencies,
        };

        let mut guard = self.inner.lock().await;
        guard.page_cache.insert(url_string, page.clone());
        Ok(vec![page])
    }

    async fn fetch_raw(&self, url_str: &str) -> Result<String, ScoutError> {
        let url = Url::parse(url_str)?;
        let body = retry_with_backoff(|| async {
            let mut client = self.client.lock().await;
            client.fetch(&url).await
        })
        .await
        .map_err(|e| ScoutError::Network(e.to_string()))?;
        Ok(body.body)
    }

    pub async fn crawl_multiple(&self, urls: &[String]) -> Vec<ScoutPage> {
        use futures::stream::{FuturesUnordered, StreamExt};
        let mut results = Vec::new();
        let mut stream = urls
            .iter()
            .map(|u| self.crawl(u, 0))
            .collect::<FuturesUnordered<_>>();
        while let Some(res) = stream.next().await {
            match res {
                Ok(pages) => results.extend(pages),
                Err(e) => tracing::warn!("crawl failed: {}", e),
            }
        }
        results
    }

    pub async fn crawl_deep(&self, seed_url: &str) -> Vec<ScoutPage> {
        let mut visited = Vec::new();
        let mut queue: Vec<(String, u32)> = vec![(seed_url.to_string(), 0)];

        while let Some((url, depth)) = queue.pop() {
            match self.crawl(&url, depth).await {
                Ok(pages) => {
                    for page in &pages {
                        if depth < self.config.max_depth {
                            for link in &page.links {
                                queue.push((link.clone(), depth + 1));
                            }
                        }
                    }
                    visited.extend(pages);
                }
                Err(e) => tracing::warn!("deep crawl error at {}: {}", url, e),
            }
        }
        visited
    }

    /// Fetch and parse a sitemap.xml from a URL, returning discovered URLs.
    pub async fn fetch_sitemap(&self, sitemap_url: &str) -> Result<Vec<SitemapUrl>, ScoutError> {
        let raw = self.fetch_raw(sitemap_url).await?;
        Ok(sitemap::parse_sitemap(&raw))
    }

    fn resolve_links(&self, base: &Url, raw: &[String]) -> Vec<String> {
        let base_host = base.host_str().unwrap_or_default().to_lowercase();
        raw.iter()
            .filter_map(|raw_link| {
                let resolved = base.join(raw_link).ok()?;
                let resolved_host = resolved.host_str().unwrap_or_default().to_lowercase();
                if !self.config.follow_external_links && resolved_host != base_host {
                    return None;
                }
                if let Some(ref allowed) = self.config.allowed_domains {
                    if !allowed.contains(&resolved_host) {
                        return None;
                    }
                }
                if resolved.scheme() != "http" && resolved.scheme() != "https" {
                    return None;
                }
                Some(resolved.to_string())
            })
            .collect()
    }

    #[must_use]
    pub fn extract_links(html: &str) -> Vec<String> {
        extractor::extract_links(html)
    }

    /// Return a snapshot of current metrics.
    pub async fn metrics(&self) -> ScoutMetrics {
        let mut guard = self.inner.lock().await;
        ScoutMetrics {
            pages_crawled: AtomicU64::new(guard.metrics.pages_crawled.load(Ordering::Relaxed)),
            pages_cached: AtomicU64::new(guard.metrics.pages_cached.load(Ordering::Relaxed)),
            requests_failed: AtomicU64::new(guard.metrics.requests_failed.load(Ordering::Relaxed)),
            robots_blocked: AtomicU64::new(guard.metrics.robots_blocked.load(Ordering::Relaxed)),
            domains_throttled: AtomicU64::new(
                guard.metrics.domains_throttled.load(Ordering::Relaxed),
            ),
        }
    }

    /// Lazily initialize the headless browser. Call once at startup
    /// if `config.js_rendering` is true. No-op if already initialized
    /// or if `js_rendering` is false.
    pub async fn init_headless(&mut self) -> Result<(), crate::headless::HeadlessError> {
        if self.config.js_rendering && self.headless.is_none() {
            let browser = crate::headless::HeadlessBrowser::launch().await?;
            self.headless = Some(Arc::new(browser));
        }
        Ok(())
    }

    /// Capture a screenshot of `url` via the headless browser.
    /// Returns `Ok(None)` if no headless browser is available.
    pub async fn capture_screenshot_for(
        &self,
        url: &str,
        viewport_w: u32,
        viewport_h: u32,
        full_page: bool,
    ) -> Result<Option<Vec<u8>>, ScoutError> {
        let Some(browser) = &self.headless else {
            return Ok(None);
        };
        let session = browser
            .new_session(viewport_w, viewport_h)
            .await
            .map_err(|e| ScoutError::Network(format!("headless session: {e}")))?;
        let _ = session
            .fetch_rendered(url, self.config.js_render_timeout_ms)
            .await
            .map_err(|e| ScoutError::Network(format!("navigation: {e}")))?;
        let bytes = session
            .screenshot(full_page)
            .await
            .map_err(|e| ScoutError::Network(format!("screenshot: {e}")))?;
        Ok(Some(bytes))
    }
}
