//! Discovery module - integrated with AVID for web and document exploration.

use avid_scout::Scout;
use avid_vision::Vision;
use crate::bus::{Bus, Message};
use std::sync::Arc;
use tracing::info;

pub struct DiscoveryService {
    scout: Scout,
    vision: Vision,
    bus: Arc<Bus>,
}

impl DiscoveryService {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self {
            scout: Scout::default(),
            vision: Vision::default(),
            bus,
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        info!("DiscoveryService (AVID) started");
        Ok(())
    }

    pub async fn discover_web(&self, url: &str) -> anyhow::Result<()> {
        info!("Discovering web content at: {}", url);
        let result = self.scout.crawl(url).await?;

        self.bus.publish(Message::SynergyDetection {
            module: "discovery".into(),
            description: format!("AVID Scout discovered content at {}", url),
        });

        Ok(())
    }
}
