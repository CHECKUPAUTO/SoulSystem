use crate::providers::{
    ChannelProvider, IncomingMessage, MessageHandler, OutgoingMessage,
    Result,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

pub struct WebhookProvider {
    endpoint_url: String,
    secret: String,
    message_handler: RwLock<Option<Arc<dyn MessageHandler>>>,
}

impl WebhookProvider {
    pub fn new(endpoint_url: String, secret: String) -> Self {
        Self {
            endpoint_url,
            secret,
            message_handler: RwLock::new(None),
        }
    }
}

#[async_trait]
impl ChannelProvider for WebhookProvider {
    fn name(&self) -> &str {
        "webhook"
    }

    fn is_configured(&self) -> bool {
        !self.endpoint_url.is_empty() && !self.secret.is_empty()
    }

    async fn start(&self, handler: Arc<dyn MessageHandler>) -> Result<()> {
        *self.message_handler.write().await = Some(handler);
        info!("Webhook provider started for endpoint: {}", self.endpoint_url);
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        *self.message_handler.write().await = None;
        info!("Webhook provider stopped");
        Ok(())
    }

    async fn send_message(&self, msg: OutgoingMessage) -> Result<String> {
        info!("Sending webhook message to {}", msg.chat_id);
        Ok(format!("webhook_msg_{}", chrono::Utc::now().timestamp_millis()))
    }

    async fn get_chat_history(
        &self,
        _chat_id: &str,
        _limit: usize,
    ) -> Result<Vec<IncomingMessage>> {
        Ok(Vec::new())
    }
}
