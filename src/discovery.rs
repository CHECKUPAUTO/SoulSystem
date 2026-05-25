//! Discovery module - integrated with AVID for web and document exploration.

#[cfg(feature = "avid")]
use avid_scout::Scout;
#[cfg(feature = "avid")]
use avid_vision::Vision;
use crate::bus::{Bus, Message};
use std::sync::Arc;
use tracing::info;

pub struct DiscoveryService {
    #[cfg(feature = "avid")]
    scout: Scout,
    #[cfg(feature = "avid")]
    vision: Vision,
    bus: Arc<Bus>,
}

impl DiscoveryService {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self {
            #[cfg(feature = "avid")]
            scout: Scout::default(),
            #[cfg(feature = "avid")]
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
        #[cfg(feature = "avid")]
        let result = self.scout.crawl(url).await?;
        #[cfg(not(feature = "avid"))]
        let result: anyhow::Result<()> = Ok(());

        self.bus.publish(Message::SynergyDetection {
            module: "discovery".into(),
            description: format!("AVID Scout discovered content at {}", url),
        });

        Ok(())
    }
}
