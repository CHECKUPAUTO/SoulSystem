//! Agent swarming — launch 5 ScoutEngine instances with different strategies
//! and fuse their results into a single SwarmReport.

use std::collections::HashSet;
use tracing::{info, warn};

use crate::{ScoutConfig, ScoutEngine, ScoutPage};

/// Pre-defined exploration strategies for the scout swarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwarmStrategy {
    /// Focus on discovering REST / GraphQL / OpenAPI endpoints.
    ApiDiscovery,
    /// Focus on academic papers, arXiv, and PDF repositories.
    Papers,
    /// Focus on GitHub trending repos, release notes, and READMEs.
    GitHubTrends,
    /// Focus on documentation sites, wikis, and knowledge bases.
    Docs,
    /// Focus on security advisories, CVEs, and vulnerability databases.
    Security,
}

impl SwarmStrategy {
    /// Seed URLs for each strategy.
    pub fn seed_urls(&self) -> Vec<String> {
        match self {
            Self::ApiDiscovery => vec![
                "https://api.github.com".into(),
                "https://rapidapi.com/hub".into(),
            ],
            Self::Papers => vec![
                "https://arxiv.org/list/cs/recent".into(),
                "https://paperswithcode.com".into(),
            ],
            Self::GitHubTrends => vec![
                "https://github.com/trending".into(),
                "https://github.com/trending/rust".into(),
            ],
            Self::Docs => vec![
                "https://doc.rust-lang.org".into(),
                "https://docs.python.org/3".into(),
            ],
            Self::Security => vec![
                "https://cve.mitre.org".into(),
                "https://nvd.nist.gov".into(),
            ],
        }
    }

    /// Strategy-specific configuration tweaks.
    pub fn config(&self) -> ScoutConfig {
        let mut cfg = ScoutConfig::default();
        match self {
            Self::ApiDiscovery => {
                cfg.max_depth = 3;
                cfg.rate_limit_ms = 300;
                cfg.follow_external_links = true;
            }
            Self::Papers => {
                cfg.max_depth = 2;
                cfg.rate_limit_ms = 1000;
                cfg.follow_external_links = false;
            }
            Self::GitHubTrends => {
                cfg.max_depth = 2;
                cfg.rate_limit_ms = 500;
                cfg.js_rendering = true;
            }
            Self::Docs => {
                cfg.max_depth = 3;
                cfg.rate_limit_ms = 200;
                cfg.follow_external_links = true;
            }
            Self::Security => {
                cfg.max_depth = 2;
                cfg.rate_limit_ms = 800;
                cfg.follow_external_links = false;
            }
        }
        cfg
    }
}

/// Result of a single scout in the swarm.
#[derive(Debug, Clone)]
pub struct SwarmShard {
    pub strategy: SwarmStrategy,
    pub pages: Vec<ScoutPage>,
    pub error: Option<String>,
}

/// Fused report from the entire swarm.
#[derive(Debug, Clone, Default)]
pub struct SwarmReport {
    pub shards: Vec<SwarmShard>,
    pub total_pages: usize,
    pub unique_domains: HashSet<String>,
}

/// Orchestrate 5 scouts in parallel.
pub struct ScoutSwarm;

impl ScoutSwarm {
    /// Launch the full swarm and wait for every shard to finish.
    pub async fn explore_all() -> SwarmReport {
        let strategies = [
            SwarmStrategy::ApiDiscovery,
            SwarmStrategy::Papers,
            SwarmStrategy::GitHubTrends,
            SwarmStrategy::Docs,
            SwarmStrategy::Security,
        ];

        let mut handles = Vec::with_capacity(strategies.len());
        for strat in strategies {
            handles.push(tokio::spawn(Self::run_strategy(strat)));
        }

        let mut report = SwarmReport::default();
        for h in handles {
            match h.await {
                Ok(shard) => {
                    for page in &shard.pages {
                        if let Ok(url) = url::Url::parse(&page.url) {
                            if let Some(host) = url.host_str() {
                                report.unique_domains.insert(host.to_string());
                            }
                        }
                    }
                    report.total_pages += shard.pages.len();
                    report.shards.push(shard);
                }
                Err(e) => {
                    warn!("Swarm task panicked: {}", e);
                }
            }
        }
        info!(
            total_pages = report.total_pages,
            unique_domains = report.unique_domains.len(),
            "Swarm exploration complete"
        );
        report
    }

    /// Run a single strategy.
    async fn run_strategy(strategy: SwarmStrategy) -> SwarmShard {
        let cfg = strategy.config();
        let engine = ScoutEngine::with_config(cfg);
        let seeds = strategy.seed_urls();
        info!(strategy = ?strategy, seeds = seeds.len(), "Launching scout shard");

        let pages = engine.crawl_multiple(&seeds).await;
        SwarmShard {
            strategy,
            pages,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_seeds() {
        let s = SwarmStrategy::ApiDiscovery;
        assert!(!s.seed_urls().is_empty());
    }

    #[test]
    fn test_swarm_report_default() {
        let r = SwarmReport::default();
        assert_eq!(r.total_pages, 0);
        assert!(r.unique_domains.is_empty());
    }
}
