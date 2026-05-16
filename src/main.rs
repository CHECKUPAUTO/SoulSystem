//! SoulSystem — Point d'entrée principal.
//!
//! Charge la configuration, initialise le bus de messages, puis lance les
//! modules selon les flags passés en ligne de commande.
//!
//! Usage :
//!   soulsystem [--dev] [--mock]

use anyhow::Result;
use soulsystem::bus::Bus;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Chargement de la configuration centralisée
    let settings = soulsystem::config::Settings::new()?;
    info!(
        "Répertoire de configuration : {:?}",
        settings.paths.config_dir
    );
    info!(
        "Répertoire de données       : {:?}",
        settings.paths.data_dir
    );
    info!(
        "Répertoire de logs           : {:?}",
        settings.paths.log_dir
    );

    // Création des répertoires si nécessaire
    for dir in [
        &settings.paths.config_dir,
        &settings.paths.data_dir,
        &settings.paths.log_dir,
    ] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            warn!("Impossible de créer {:?}: {}", dir, e);
        }
    }

    // Arguments
    let args: Vec<String> = std::env::args().collect();
    let dev_mode = args.contains(&"--dev".to_string());

    // Tokio runtime
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        info!(
            "SoulSystem démarré{}",
            if dev_mode { " (mode dev)" } else { "" }
        );

        let bus = Arc::new(Bus::new(256));

        // Modules actifs
        let _soul_memory = match soulsystem::soul_memory::SoulMemory::new() {
            Ok(m) => { info!("SoulMemory: active (sled)"); Arc::new(m) }
            Err(e) => { warn!("SoulMemory: non disponible - {}", e); return Ok(()); }
        };

        match soulsystem::telemetry::init_telemetry() {
            Ok(_) => info!("Telemetry: active (OTLP)"),
            Err(e) => warn!("Telemetry: non disponible - {}", e),
        }

        let auth_keys = soulsystem::code_signing::AuthorizedKeys::load()
            .unwrap_or_else(|_| soulsystem::code_signing::AuthorizedKeys::new());
        let _auth_keys = Arc::new(auth_keys);
        info!("CodeSigning: actif ({} clés)", _auth_keys.len());

        let mut audit = soulsystem::audit_log::AuditLog::open("/var/log/soulsystem/audit.sled");
        match &mut audit {
            Ok(ref mut log) => {
                info!("AuditLog: actif");
                let _ = log.log("system", "startup", "SoulSystem démarré");
            }
            Err(e) => warn!("AuditLog: non disponible - {}", e),
        }

        if dev_mode {
            info!("Dashboard développeur actif sur 127.0.0.1:9090");
            #[cfg(feature = "dev")]
            {
                let bus_for_dash = bus.clone();
                tokio::spawn(async move {
                    if let Err(e) = soulsystem::dev_dashboard::run(bus_for_dash).await {
                        tracing::error!("Dashboard error: {}", e);
                    }
                });
            }
        }

        // Maintient le processus en vie
        tokio::signal::ctrl_c().await?;
        info!("SoulSystem arrêté proprement.");
        Ok::<_, anyhow::Error>(())
    })?;

    Ok(())
}
