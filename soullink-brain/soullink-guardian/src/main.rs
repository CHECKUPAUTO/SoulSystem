#![allow(missing_docs, dead_code, unsafe_code)]

mod config;
mod decision;
mod emergency;
mod error;
mod homeosync;
mod ipc_server;
mod kmsg_reader;
mod metrics_server;
mod pacing;
mod thermal;
mod ws_client;

use std::{path::PathBuf, sync::Arc};

use anyhow::Context;
use arc_swap::ArcSwap;
use tokio::{
    runtime, signal,
    sync::{broadcast, mpsc},
    time,
};
use tracing::{info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use soullink_core::{GuardianEvent, SystemSnapshot};
use soullink_nvml::NvmlBridge;

use crate::{
    config::GuardianConfig, decision::DecisionEngine, emergency::EmergencyReflex,
    homeosync::HomeoSync, metrics_server::MetricsServer, thermal::ThermalGuard,
    ws_client::OrchestratorClient,
};

fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("soullink_guardian=info,core_guardian=info,warn"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json())
        .init();

    info!("SoulLink Guardian V13.5 — Steel & Light | Reactor + MPSC + ArcSwap | NVML-only (no subprocess)");

    // T430 has 2× E5-2683 v4 = 64 threads. Guardian workload is I/O-bound; 8 workers
    // are plenty and leave CPU headroom for orchestrator + brains.
    let rt = runtime::Builder::new_multi_thread()
        .worker_threads(8)
        .thread_name("sl-guardian")
        .thread_stack_size(2 * 1024 * 1024)
        .enable_all()
        .build()
        .context("tokio runtime")?;

    rt.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let config = GuardianConfig::load()?;
    let nvml = Arc::new(
        NvmlBridge::init(config.gpu_device_index)
            .context("NVML init — check nvidia driver presence + CAP_SYS_ADMIN on service")?,
    );

    let state: Arc<ArcSwap<SystemSnapshot>> =
        Arc::new(ArcSwap::from(Arc::new(SystemSnapshot::default())));
    let (tx, rx) = mpsc::channel::<GuardianEvent>(256);
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    let thermal = ThermalGuard::new(
        nvml.clone(),
        tx.clone(),
        shutdown_tx.subscribe(),
        config.thermal_threshold_c,
        config.thermal_interval,
    );
    let homeosync = HomeoSync::new(
        nvml.clone(),
        state.clone(),
        tx.clone(),
        shutdown_tx.subscribe(),
        config.homeo_interval,
    );
    let emergency = EmergencyReflex::with_options(
        nvml.clone(),
        tx.clone(),
        shutdown_tx.subscribe(),
        PathBuf::from(&config.reset_script),
        config.pcie_hang_threshold_ms,
        config.hang_detector_enabled,
    );
    let ws_client = OrchestratorClient::new(config.orchestrator_url.clone(), state.clone());
    let metrics = MetricsServer::new(config.metrics_port, state.clone());
    let mut decision =
        DecisionEngine::new(rx, ws_client, state.clone(), config.clone(), nvml.clone());

    // Phase 2b: dedicated XID reader off /dev/kmsg
    let kmsg = kmsg_reader::KmsgReader::new(tx.clone(), shutdown_tx.subscribe());

    // Phase 3: Unix Domain Socket server for the Orchestrator to consume
    // SystemSnapshot frames in real time. Coexists with the HTTP fallback.
    let ipc_path = std::env::var("SOULLINK_IPC_SOCKET")
        .unwrap_or_else(|_| soullink_core::ipc::DEFAULT_SOCKET_PATH.to_string());
    let ipc = ipc_server::IpcServer::new(ipc_path, state.clone(), shutdown_tx.subscribe());

    let t1 = tokio::spawn(async move { thermal.run().await });
    let t2 = tokio::spawn(async move { homeosync.run().await });
    let t3 = tokio::spawn(async move { emergency.run().await });
    let tk = tokio::spawn(async move { kmsg.run().await });
    let ti = tokio::spawn(async move { ipc.run().await });
    let m = tokio::spawn(async move { metrics.run().await });
    let d = tokio::spawn(async move { decision.run().await });

    tokio::select! {
        _ = signal::ctrl_c() => info!("SIGINT — graceful shutdown"),
        _ = async {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut sig = signal(SignalKind::terminate()).unwrap();
                sig.recv().await;
            }
            #[cfg(not(unix))]
            std::future::pending::<()>().await;
        } => info!("SIGTERM — graceful shutdown"),
    }

    let _ = shutdown_tx.send(());
    info!("shutdown broadcast sent");

    let timeout = time::Duration::from_secs(8);
    tokio::select! {
        _ = async { let _ = tokio::join!(t1, t2, t3, tk, ti, m, d); } => info!("all actors stopped"),
        _ = time::sleep(timeout) => warn!("shutdown timeout — forced exit"),
    }

    info!("SoulLink Guardian V13.5 shutdown complete");
    Ok(())
}
