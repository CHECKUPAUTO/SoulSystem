use tokio::sync::{broadcast, RwLock};
use std::collections::HashMap;
use uuid::Uuid;
#[derive(Clone, Debug)]
pub enum StreamEvent { Meta { assistant_message_id: Uuid }, Delta(String), Usage { input_tokens: u32, output_tokens: u32 }, Done, Error(String) }
pub struct StreamBroker {
    senders: RwLock<HashMap<Uuid, broadcast::Sender<StreamEvent>>>,
    cancels: RwLock<HashMap<Uuid, tokio_util::sync::CancellationToken>>,
}
impl StreamBroker {
    pub fn new() -> Self { Self { senders: Default::default(), cancels: Default::default() } }
    pub async fn register(&self, msg_id: Uuid) -> (broadcast::Sender<StreamEvent>, tokio_util::sync::CancellationToken) {
        let (tx, _) = broadcast::channel(128);
        let cancel = tokio_util::sync::CancellationToken::new();
        self.senders.write().await.insert(msg_id, tx.clone());
        self.cancels.write().await.insert(msg_id, cancel.clone());
        (tx, cancel)
    }
    pub async fn subscribe(&self, msg_id: Uuid) -> Option<broadcast::Receiver<StreamEvent>> { self.senders.read().await.get(&msg_id).map(|tx| tx.subscribe()) }
    pub async fn cancel(&self, msg_id: Uuid) { if let Some(c) = self.cancels.read().await.get(&msg_id) { c.cancel(); } }
    pub async fn cleanup(&self, msg_id: Uuid) { self.senders.write().await.remove(&msg_id); self.cancels.write().await.remove(&msg_id); }
}
