//! Discovery module - integrated with AVID for web and document exploration.

use crate::bus::{Bus, Message};
use std::sync::Arc;
use tracing::info;

#[cfg(feature = "avid")]
use avid_bridge::{Scout, Vision};

/// Stub type utilisé quand la feature `avid` n'est pas activée
#[cfg(not(feature = "avid"))]
pub struct Scout;
#[cfg(not(feature = "avid"))]
impl Scout {
    pub async fn crawl(&self, _url: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Stub type utilisé quand la feature `avid` n'est pas activée
#[cfg(not(feature = "avid"))]
pub struct Vision;
#[cfg(not(feature = "avid"))]
impl Vision {
    pub fn default() -> Self { Self }
}

pub struct DiscoveryService {
    _scout: Scout,
    _vision: Vision,
    bus: Arc<Bus>,
}

impl DiscoveryService {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self {
            _scout: Scout::default(),
            _vision: Vision::default(),
            bus,
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("DiscoveryService started");
        Ok(())
    }

    pub async fn discover_web(&self, url: &str) -> anyhow::Result<()> {
        info!("Discovering web content at: {}", url);
        self.bus.publish(Message::SynergyDetection {
            module: "discovery".into(),
            description: format!("Discovery discovered content at {}", url),
        });
        Ok(())
    }
}