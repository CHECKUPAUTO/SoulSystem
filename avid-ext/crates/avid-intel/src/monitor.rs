use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::nvd::NvdClient;
use crate::types::{CveAlert, MonitorConfig, MonitorState};

/// Moteur de surveillance qui alimente SYNERGIE
///
/// Boucle périodique qui checke les nouvelles CVEs et génère des alertes.
/// Conçu pour être exécuté comme tâche tokio de fond.
pub struct IntelMonitor {
    config: MonitorConfig,
    state: Arc<Mutex<MonitorState>>,
    nvd_client: NvdClient,
    running: Arc<AtomicBool>,
}

impl IntelMonitor {
    /// Crée un nouveau moniteur avec la configuration donnée
    pub fn new(config: MonitorConfig) -> Self {
        let nvd_client = NvdClient::new(config.nvd_api_key.clone());
        Self {
            config,
            state: Arc::new(Mutex::new(MonitorState::new())),
            nvd_client,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Démarre la boucle de surveillance (non-blocking)
    /// Retourne un handle pour arrêter le moniteur
    pub async fn start(&self) -> anyhow::Result<tokio::sync::watch::Receiver<bool>> {
        let (tx, rx) = tokio::sync::watch::channel(false);

        let state = Arc::clone(&self.state);
        let nvd_client = NvdClient::new(self.config.nvd_api_key.clone());
        let running = Arc::clone(&self.running);
        let interval = self.config.check_interval_minutes;
        let max_results = self.config.max_results_per_check;

        running.store(true, Ordering::SeqCst);
        info!("IntelMonitor started (interval: {} min, max: {} results)", interval, max_results);

        tokio::spawn(async move {
            loop {
                if !running.load(Ordering::SeqCst) {
                    info!("IntelMonitor stopping...");
                    break;
                }

                match check_for_new(&nvd_client, &state, max_results).await {
                    Ok(alerts) => {
                        if !alerts.is_empty() {
                            info!("IntelMonitor: {} new alerts generated", alerts.len());
                            for alert in &alerts {
                                info!("ALERT: {}", alert.summary);
                            }
                        } else {
                            info!("IntelMonitor: no new CVEs found");
                        }
                    }
                    Err(e) => {
                        error!("IntelMonitor check failed: {}", e);
                    }
                }

                // Attendre l'intervalle ou un signal d'arrêt
                tokio::time::sleep(Duration::from_secs(interval * 60)).await;

                // Vérifier si on doit s'arrêter
                if tx.is_closed() {
                    break;
                }
            }
            info!("IntelMonitor stopped");
        });

        Ok(rx)
    }

    /// Arrête le moniteur
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("IntelMonitor stop signal sent");
    }

    /// Exécute un check unique (pour usage one-shot)
    pub async fn check_once(&self) -> anyhow::Result<Vec<CveAlert>> {
        check_for_new(&self.nvd_client, &self.state, self.config.max_results_per_check).await
    }

    /// Retourne l'état actuel du moniteur
    pub async fn get_state(&self) -> MonitorState {
        self.state.lock().await.clone()
    }

    /// Vérifie si le moniteur tourne
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Vérifie les nouvelles CVEs depuis le dernier check
/// et génère des alertes formatées pour SYNERGIE
async fn check_for_new(
    nvd_client: &NvdClient,
    state: &Arc<Mutex<MonitorState>>,
    max_results: usize,
) -> anyhow::Result<Vec<CveAlert>> {
    let mut guard = state.lock().await;
    let mut alerts = Vec::new();

    // Si c'est le premier check, on récupère les récents
    let cves = if let Some(last_check) = guard.last_check {
        info!("Checking for CVEs since {}", last_check.to_rfc3339());
        nvd_client.fetch_since(&last_check, max_results).await?
    } else {
        info!("First check — fetching recent CVEs");
        nvd_client.fetch_recent(max_results).await?
    };

    // Filtrer les CVEs déjà connues
    for cve in &cves {
        if !guard.known_cves.contains(&cve.id) {
            let alert = CveAlert::new(cve.clone(), "NVD".to_string());
            alerts.push(alert);
            guard.known_cves.push(cve.id.clone());
        }
    }

    // Mettre à jour le timestamp du dernier check
    guard.last_check = Some(Utc::now());

    info!(
        "Check complete: {} new alerts out of {} CVEs checked",
        alerts.len(),
        cves.len()
    );

    // Limiter le nombre de known_cves pour éviter la fuite mémoire
    if guard.known_cves.len() > 10_000 {
        let split_at = guard.known_cves.len() - 5_000;
        guard.known_cves.drain(..split_at);
        info!("Trimmed known_cves to last 5000 entries");
    }

    Ok(alerts)
}

/// Génère un rapport formaté pour SYNERGIE
pub fn format_synergie_report(alerts: &[CveAlert]) -> String {
    if alerts.is_empty() {
        return "No new vulnerabilities detected.".to_string();
    }

    let critical_count = alerts.iter().filter(|a| a.cve.severity == "CRITICAL").count();
    let high_count = alerts.iter().filter(|a| a.cve.severity == "HIGH").count();
    let other_count = alerts.len() - critical_count - high_count;

    let mut report = format!(
        "## SYNERGIE Intel Report\n\n\
         Summary: {} new alerts ({} critical, {} high, {} other)\n\n",
        alerts.len(),
        critical_count,
        high_count,
        other_count,
    );

    // Trier par sévérité (critical first)
    let mut sorted = alerts.to_vec();
    sorted.sort_by(|a, b| {
        let a_score = a.cve.cvss_score.unwrap_or(0.0);
        let b_score = b.cve.cvss_score.unwrap_or(0.0);
        b_score.partial_cmp(&a_score).unwrap_or(std::cmp::Ordering::Equal)
    });

    for alert in sorted.iter().take(20) {
        // Top 20
        let cvss = alert
            .cve
            .cvss_score
            .map(|s| format!("CVSS: {s:.1}"))
            .unwrap_or_else(|| "CVSS: N/A".to_string());
        report.push_str(&format!(
            "- **{}** [{}] {} — {}\n  _{}_\n",
            alert.cve.id,
            alert.cve.severity,
            cvss,
            alert.cve.description.chars().take(150).collect::<String>(),
            alert.cve.references.first().unwrap_or(&"No references".to_string()),
        ));
    }

    if alerts.len() > 20 {
        report.push_str(&format!("\n... and {} more alerts", alerts.len() - 20));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitor_config_default() {
        let config = MonitorConfig::default();
        assert_eq!(config.check_interval_minutes, 60);
        assert_eq!(config.max_results_per_check, 50);
        assert!(config.nvd_api_key.is_none());
    }

    #[tokio::test]
    async fn test_monitor_new() {
        let config = MonitorConfig::default();
        let monitor = IntelMonitor::new(config);
        assert!(!monitor.is_running());
    }

    #[tokio::test]
    async fn test_monitor_check_once() {
        let config = MonitorConfig {
            nvd_api_key: None,
            check_interval_minutes: 60,
            max_results_per_check: 5,
        };
        let monitor = IntelMonitor::new(config);
        let alerts = monitor.check_once().await.unwrap();
        // Peut être vide si pas de réseau, mais ne panique pas
    }

    #[tokio::test]
    async fn test_monitor_state() {
        let config = MonitorConfig::default();
        let monitor = IntelMonitor::new(config);
        let state = monitor.get_state().await;
        assert!(state.last_check.is_none());
        assert!(state.known_cves.is_empty());
    }

    #[test]
    fn test_format_report_empty() {
        let report = format_synergie_report(&[]);
        assert!(report.contains("No new vulnerabilities"));
    }

    #[test]
    fn test_format_report_with_alerts() {
        use crate::types::CveEntry;
        let cve = CveEntry::new("CVE-2024-TEST".to_string(), "Test vulnerability".to_string());
        let alerts = vec![CveAlert::new(cve, "NVD".to_string())];
        let report = format_synergie_report(&alerts);
        assert!(report.contains("CVE-2024-TEST"));
        assert!(report.contains("SYNERGIE Intel Report"));
    }

    #[tokio::test]
    async fn test_monitor_start_stop() {
        let config = MonitorConfig {
            nvd_api_key: None,
            check_interval_minutes: 1, // 1 minute interval
            max_results_per_check: 5,
        };
        let monitor = IntelMonitor::new(config);
        let rx = monitor.start().await.unwrap();
        assert!(monitor.is_running());
        monitor.stop();
        assert!(!monitor.is_running());
        // Nettoyer le watch channel
        drop(rx);
    }
}
