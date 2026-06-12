# CodeWiki MCP Fallback Mode

**Source:** OpenEvolve Night Cycle Report 2026-04-12 (0019)
**Purpose:** Ensure CodeWiki availability when MCP server is unavailable

## Problem Statement

IronReview depends on CodeWiki MCP for pattern matching. If the MCP server is down, IronReview cannot function.

## Solution: Three-Tier Fallback

```rust
// ironreview/src/codewiki/mode.rs

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodeWikiMode {
    /// Direct MCP server connection (preferred)
    Direct,
    /// Local pattern cache from last successful fetch
    Cached,
    /// Static rule-based patterns (always available)
    Fallback,
}

pub struct CodeWikiClient {
    mode: CodeWikiMode,
    cache: Option<PatternCache>,
    fallback_rules: Vec<StaticPattern>,
}

impl CodeWikiClient {
    pub async fn new(config: &Config) -> Result<Self, CodeWikiError> {
        // Try MCP first
        if let Ok(client) = McpClient::connect(&config.mcp_endpoint).await {
            return Ok(Self {
                mode: CodeWikiMode::Direct,
                cache: None,
                fallback_rules: Self::load_fallback_rules(),
            });
        }
        
        // Try cache
        if let Ok(cache) = PatternCache::load(&config.cache_path) {
            if !cache.is_stale(config.cache_ttl) {
                return Ok(Self {
                    mode: CodeWikiMode::Cached,
                    cache: Some(cache),
                    fallback_rules: Self::load_fallback_rules(),
                });
            }
        }
        
        // Fallback to static rules
        Ok(Self {
            mode: CodeWikiMode::Fallback,
            cache: None,
            fallback_rules: Self::load_fallback_rules(),
        })
    }
    
    pub async fn match_patterns(
        &self,
        code: &str,
    ) -> Result<Vec<PatternMatch>, CodeWikiError> {
        match self.mode {
            CodeWikiMode::Direct => {
                self.mcp_match(code).await
            }
            CodeWikiMode::Cached => {
                self.cache_match(code)
            }
            CodeWikiMode::Fallback => {
                self.fallback_match(code)
            }
        }
    }
}
```

## Fallback Patterns

```rust
// ironreview/src/codewiki/fallback_patterns.rs

pub static FALLBACK_PATTERNS: &[StaticPattern] = &[
    // Provider Capability Negotiation
    StaticPattern {
        name: "provider_capability_negotiation",
        detect: |code| code.contains("providerOptions") && code.contains("capabilities"),
        severity: PatternSeverity::Info,
    },
    
    // Extension Event Dispatcher
    StaticPattern {
        name: "extension_event_dispatcher",
        detect: |code| code.contains("monitor.") && code.contains("dispatcher"),
        severity: PatternSeverity::Architecture,
    },
    
    // Barrel Avoidance
    StaticPattern {
        name: "barrel_avoidance",
        detect: |code| {
            code.contains("from '../../channels/plugins/index'") ||
            code.contains("@openclaw/plugin-sdk")
        },
        severity: PatternSeverity::Performance,
    },
    
    // Static Lookup Short-Circuit
    StaticPattern {
        name: "static_lookup_short_circuit",
        detect: |code| code.contains("STATIC_") && code.contains("||"),
        severity: PatternSeverity::Performance,
    },
];

pub fn fallback_match(code: &str) -> Vec<PatternMatch> {
    FALLBACK_PATTERNS
        .iter()
        .filter(|p| (p.detect)(code))
        .map(|p| PatternMatch {
            pattern: p.name.to_string(),
            confidence: 0.7, // Lower confidence than MCP
            source: PatternSource::Fallback,
        })
        .collect()
}
```

## Cache Management

```rust
// ironreview/src/codewiki/cache.rs

pub struct PatternCache {
    patterns: Vec<CachedPattern>,
    fetched_at: DateTime<Utc>,
}

impl PatternCache {
    pub async fn update(&mut self,
        mcp_client: &McpClient,
    ) -> Result<(), CodeWikiError> {
        let patterns = mcp_client.fetch_patterns().await?;
        self.patterns = patterns.into_iter()
            .map(|p| CachedPattern::from(p))
            .collect();
        self.fetched_at = Utc::now();
        self.save()
    }
    
    pub fn is_stale(&self,
        ttl: Duration,
    ) -> bool {
        Utc::now() - self.fetched_at > ttl
    }
    
    pub fn save(&self,
    ) -> Result<(), CodeWikiError> {
        let json = serde_json::to_string(self)?;
        fs::write(CACHE_PATH, json)?;
        Ok(())
    }
    
    pub fn load(path: &Path) -> Result<Self, CodeWikiError> {
        let json = fs::read_to_string(path)?;
        let cache: PatternCache = serde_json::from_str(&json)?;
        Ok(cache)
    }
}
```

## Mode Switching

```rust
impl CodeWikiClient {
    /// Attempt to upgrade to Direct mode
    pub async fn try_upgrade(&mut self,
    ) {
        if self.mode == CodeWikiMode::Direct {
            return;
        }
        
        if let Ok(client) = McpClient::connect(&self.config.mcp_endpoint).await {
            self.mode = CodeWikiMode::Direct;
            
            // Update cache for next time
            if let Err(e) = self.update_cache().await {
                log::warn!("Failed to update cache: {}", e);
            }
        }
    }
    
    /// Force fallback mode (e.g., during network issues)
    pub fn force_fallback(&mut self,
    ) {
        self.mode = CodeWikiMode::Fallback;
        log::info!("CodeWiki forced to fallback mode");
    }
}
```

## Health Monitoring

```rust
// ironreview/src/codewiki/health.rs

pub struct CodeWikiHealth {
    mode: CodeWikiMode,
    last_successful_fetch: Option<DateTime<Utc>>,
    pattern_count: usize,
}

impl CodeWikiHealth {
    pub fn to_metrics(&self,
    ) -> Vec<Metric> {
        vec![
            Metric::gauge("codewiki_mode", self.mode as i32),
            Metric::gauge("codewiki_patterns", self.pattern_count as f64),
            Metric::gauge("codewiki_cache_age_seconds",
                self.cache_age().as_secs_f64()),
        ]
    }
}
```

## Configuration

```toml
# ironreview.toml
[codewiki]
# Endpoint configuration
mcp_endpoint = "http://localhost:8080/mcp"
cache_path = "/var/cache/ironreview/codewiki.json"
cache_ttl_seconds = 3600  # 1 hour

# Fallback behavior
fallback_on_error = true
attempt_upgrade = true
upgrade_interval_seconds = 300  # 5 minutes
```

## Benefits

| Mode | Latency | Pattern Freshness | Availability |
|------|---------|-------------------|--------------|
| Direct | ~50ms | Real-time | Requires MCP |
| Cached | ~5ms | Stale (configurable) | File system |
| Fallback | ~1ms | Static | Always |

## References

- Night Cycle Report: night_cycle_20260412_0019.md
- Related: CodeWiki pattern documentation
