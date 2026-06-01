use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::archive::Archive;
use crate::blackboard::Blackboard;
use crate::intent::{Intent, IntentResult};

#[derive(Debug, thiserror::Error)]
pub enum SoulLinkError {
    #[error("blackboard: {0}")]
    Blackboard(#[from] crate::blackboard::BlackboardError),
    #[error("archive: {0}")]
    Archive(#[from] crate::archive::ArchiveError),
}

#[derive(Debug, Clone)]
pub struct SoulLinkConfig {
    pub topic: String,
    pub reconnect_backoff: Duration,
    pub max_concurrency: usize,
}

impl Default for SoulLinkConfig {
    fn default() -> Self {
        Self {
            topic: "forge.avid".to_string(),
            reconnect_backoff: Duration::from_secs(2),
            max_concurrency: 4,
        }
    }
}

#[async_trait]
pub trait IntentExecutor: Send + Sync {
    async fn execute(&self, intent: &Intent) -> IntentResult;
}

pub struct SoulLinkAgent<B: Blackboard, A: Archive, E: IntentExecutor> {
    blackboard: Arc<B>,
    archive: Arc<A>,
    executor: Arc<E>,
    config: SoulLinkConfig,
}

impl<B, A, E> SoulLinkAgent<B, A, E>
where
    B: Blackboard + 'static,
    A: Archive + 'static,
    E: IntentExecutor + 'static,
{
    pub fn new(
        blackboard: Arc<B>,
        archive: Arc<A>,
        executor: Arc<E>,
        config: SoulLinkConfig,
    ) -> Self {
        Self {
            blackboard,
            archive,
            executor,
            config,
        }
    }

    pub async fn run(self: Arc<Self>) {
        loop {
            match self.blackboard.subscribe_intents(&self.config.topic).await {
                Ok(mut stream) => {
                    info!(topic = %self.config.topic, "soullink subscribed");
                    while let Some(intent_res) = stream.next().await {
                        let intent = match intent_res {
                            Ok(i) => i,
                            Err(e) => {
                                warn!(error = %e, "intent recv error, reconnecting");
                                break;
                            }
                        };
                        let me = Arc::clone(&self);
                        tokio::spawn(async move { me.handle(intent).await });
                    }
                }
                Err(e) => {
                    error!(error = %e, "subscribe failed");
                }
            }
            sleep(self.config.reconnect_backoff).await;
        }
    }

    async fn handle(&self, intent: Intent) {
        info!(intent_id = %intent.id, kind = ?intent.kind, "handling intent");
        let result = self.executor.execute(&intent).await;

        if let Err(e) = self
            .archive
            .store(&format!("avid:result:{}", intent.id), &result)
            .await
        {
            warn!(error = %e, "archive failed");
        }

        if let Err(e) = self.blackboard.publish_result(&result).await {
            warn!(error = %e, "publish failed");
        }
    }
}
