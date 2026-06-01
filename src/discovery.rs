//! Discovery module - integrated with AVID for web and document exploration.

use crate::bus::{Bus, Message};
#[cfg(feature = "avid")]
use avid_bridge::{Scout, Vision};
use std::sync::Arc;
use tracing::info;

pub struct DiscoveryService {
    bus: Arc<Bus>,
}

impl DiscoveryService {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self { bus }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("DiscoveryService started");
        Ok(())
    }

    pub async fn discover_web(&self, url: &str) -> anyhow::Result<()> {
        info!("Discovering web content at: {}", url);
        #[cfg(feature = "avid")]
        {
            let scout = Scout::default();
            scout.crawl(url).await?;
        }
        self.bus.publish(Message::SynergyDetection {
            module: "discovery".into(),
            description: format!("AVID Scout discovered content at {}", url),
        });
        Ok(())
    }

    #[cfg(feature = "avid")]
    pub async fn discover_vision(&self, url: &str) -> anyhow::Result<()> {
        info!("Vision discovery at: {}", url);
        let _vision = Vision::default();
        Ok(())
    }
}