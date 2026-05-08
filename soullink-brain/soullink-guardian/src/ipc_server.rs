//! Unix Domain Socket server — Guardian side.
//!
//! Listens on `/run/soullink/guardian.sock`, accepts incoming connections
//! from the orchestrator, and fans out `SystemSnapshot` updates as they
//! arrive on an `ArcSwap` (published by `HomeoSync`). One broadcast per
//! snapshot, delivered to every connected peer with length-prefixed
//! MessagePack framing.
//!
//! # Coexistence with HTTP
//!
//! This **adds** UDS alongside the existing HTTP push from `ws_client.rs`.
//! Both run concurrently; the orchestrator chooses which to accept (the
//! orchestrator's UDS connector is the primary path, the HTTP handler on
//! `/api/guardian/snapshot` remains as fallback for transitional
//! environments).
//!
//! # Lifecycle
//!
//! 1. Bind the socket (removing a stale one if present).
//! 2. Spawn an accept loop.
//! 3. Per connection: a writer task subscribes to a `tokio::sync::watch`
//!    that holds the latest snapshot. Each watch change triggers a frame
//!    write. Dead peers (I/O error on write) are dropped.
//! 4. On shutdown: unbind the socket file, drop all peer tasks.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use arc_swap::ArcSwap;
use tokio::{
    net::UnixListener,
    sync::{broadcast, watch},
};
use tracing::{debug, error, info, warn};

use soullink_core::{ipc::write_frame, SystemSnapshot};

pub struct IpcServer {
    socket_path: PathBuf,
    state:       Arc<ArcSwap<SystemSnapshot>>,
    shutdown:    broadcast::Receiver<()>,
}

impl IpcServer {
    pub fn new(
        socket_path: impl Into<PathBuf>,
        state: Arc<ArcSwap<SystemSnapshot>>,
        shutdown: broadcast::Receiver<()>,
    ) -> Self {
        Self {
            socket_path: socket_path.into(),
            state,
            shutdown,
        }
    }

    pub async fn run(mut self) {
        if let Err(e) = prepare_socket_dir(&self.socket_path).await {
            error!(%e, path = %self.socket_path.display(),
                   "IpcServer: cannot prepare socket directory");
            return;
        }

        // Remove stale socket (previous unclean shutdown)
        let _ = tokio::fs::remove_file(&self.socket_path).await;

        let listener = match UnixListener::bind(&self.socket_path) {
            Ok(l) => l,
            Err(e) => {
                error!(%e, path = %self.socket_path.display(), "UnixListener::bind failed");
                return;
            }
        };

        // Tighten permissions: only the owner may connect.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &self.socket_path,
                std::fs::Permissions::from_mode(0o600),
            );
        }

        info!(path = %self.socket_path.display(), "IpcServer listening (UDS, MsgPack frames)");

        // watch channel broadcasts the latest snapshot to all peer tasks.
        let (snap_tx, snap_rx) = watch::channel(self.state.load().as_ref().clone());

        // Periodically refresh the watch from the ArcSwap (HomeoSync updates
        // it every 5 s). We poll the ArcSwap at 1 s granularity — lower than
        // HomeoSync's period, so every real update reaches peers promptly.
        let state_clone = self.state.clone();
        let snap_tx_task = snap_tx.clone();
        let mut shutdown_watch = self.shutdown.resubscribe();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let current = state_clone.load().as_ref().clone();
                        // watch::send deduplicates identical payloads
                        let _ = snap_tx_task.send(current);
                    }
                    _ = shutdown_watch.recv() => break,
                }
            }
        });

        loop {
            tokio::select! {
                res = listener.accept() => {
                    match res {
                        Ok((stream, addr)) => {
                            debug!(?addr, "IpcServer: peer connected");
                            let rx = snap_rx.clone();
                            tokio::spawn(handle_peer(stream, rx));
                        }
                        Err(e) => {
                            warn!(%e, "IpcServer: accept failed");
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }
                    }
                }
                _ = self.shutdown.recv() => {
                    info!("IpcServer: shutdown");
                    break;
                }
            }
        }

        // Clean up the socket file
        let _ = tokio::fs::remove_file(&self.socket_path).await;
    }
}

async fn handle_peer(
    mut stream: tokio::net::UnixStream,
    mut snap_rx: watch::Receiver<SystemSnapshot>,
) {
    // Send the current snapshot immediately on connect (no wait for next
    // HomeoSync tick — new peers catch up instantly).
    let current = snap_rx.borrow().clone();
    if write_frame(&mut stream, &current).await.is_err() {
        return;
    }

    loop {
        // Await the next changed value
        if snap_rx.changed().await.is_err() {
            break; // sender dropped → guardian is shutting down
        }
        let snap = snap_rx.borrow().clone();
        if let Err(e) = write_frame(&mut stream, &snap).await {
            debug!(%e, "IpcServer: peer write failed — dropping connection");
            break;
        }
    }
}

async fn prepare_socket_dir(socket_path: &Path) -> std::io::Result<()> {
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    Ok(())
}
