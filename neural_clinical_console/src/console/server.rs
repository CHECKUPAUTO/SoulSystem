use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::TcpListener;
use tokio::io::AsyncWriteExt;
use neural_metacognition::SystemAuditor;

pub struct ClinicalStreamingServer {
    auditor: Arc<SystemAuditor>,
    is_active: Arc<AtomicBool>,
    listen_port: u16,
}

impl ClinicalStreamingServer {
    pub fn new(auditor: Arc<SystemAuditor>, port: u16) -> Self {
        Self { auditor, is_active: Arc::new(AtomicBool::new(false)), listen_port: port }
    }
    pub async fn start_streaming(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.listen_port)).await?;
        self.is_active.store(true, Ordering::SeqCst);
        loop {
            if !self.is_active.load(Ordering::SeqCst) { break; }
            let (mut socket, _) = listener.accept().await?;
            let auditor = self.auditor.clone();
            tokio::spawn(async move {
                loop {
                    let frame = auditor.get_latest();
                    let bytes = unsafe { std::slice::from_raw_parts(&frame as *const _ as *const u8, std::mem::size_of::<neural_metacognition::TelemetryFrame>()) };
                    if socket.write_all(bytes).await.is_err() { break; }
                    tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                }
            });
        }
        Ok(())
    }
    pub fn shutdown(&self) { self.is_active.store(false, Ordering::SeqCst); }
}
