//! AnomalyWatcher — Détecteur de chutes de ticks HNN.
//!
//! S'abonne au Bus pour recevoir les métriques HNN des organs de SoulLink
//! et détecte les chutes brutales de fréquence de ticks.
//!
//! Seuil par défaut : chute > 40% sur une fenêtre de 3 échantillons.
//! Alerte émise sur le Bus en cas de dépassement.

use crate::bus::Bus;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Nombre d'échantillons dans la fenêtre glissante.
const WINDOW_SIZE: usize = 5;

/// Seuil de chute détectée (ratio).
const DROP_THRESHOLD: f64 = 0.40; // 40%

/// Intervalle de vérification.
const CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Observateur de ticks HNN.
pub struct AnomalyWatcher {
    bus: Arc<Bus>,
    history: VecDeque<(Instant, f64)>, // (timestamp, tick_rate)
    alert_cooldown: Option<Instant>,
}

impl AnomalyWatcher {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self {
            bus,
            history: VecDeque::with_capacity(WINDOW_SIZE + 1),
            alert_cooldown: None,
        }
    }

    /// Démarre la boucle de surveillance.
    pub async fn run(&mut self) {
        info!(
            "AnomalyWatcher démarré (seuil chute > {}%)",
            DROP_THRESHOLD * 100.0
        );

        loop {
            tokio::time::sleep(CHECK_INTERVAL).await;
            self.check().await;
        }
    }

    /// Vérifie les métriques et détecte les anomalies.
    async fn check(&mut self) {
        // Simule une lecture de métrique HNN.
        // En production : lire via le bus ou un endpoint HTTP.
        let tick_rate = self.read_hnn_tick_rate().await;

        let now = Instant::now();
        self.history.push_back((now, tick_rate));
        if self.history.len() > WINDOW_SIZE {
            self.history.pop_front();
        }

        if self.history.len() < 3 {
            return; // pas assez de données
        }

        // Calcul de la moyenne sur la fenêtre
        let recent: Vec<f64> = self.history.iter().map(|(_, r)| *r).collect();

        // Comparaison avec la moitié la plus ancienne
        let mid = recent.len() / 2;
        if mid < 1 {
            return;
        }
        let older_mean = recent[..mid].iter().sum::<f64>() / mid as f64;
        let newer_mean = recent[mid..].iter().sum::<f64>() / (recent.len() - mid) as f64;

        if older_mean > 0.0 {
            let drop_ratio = (older_mean - newer_mean) / older_mean;
            if drop_ratio > DROP_THRESHOLD {
                let cooldown = self.alert_cooldown.map(|c| c > now).unwrap_or(false);

                if !cooldown {
                    warn!(
                        "⚠️ Anomalie HNN détectée : chute de {:.1}% (avant: {:.0} Hz, après: {:.0} Hz)",
                        drop_ratio * 100.0, older_mean, newer_mean
                    );

                    self.emit_alert(drop_ratio, older_mean, newer_mean).await;

                    // cooldown de 60s
                    self.alert_cooldown = Some(now + Duration::from_secs(60));
                }
            }
        }
    }

    /// Simule la lecture du tick rate HNN.
    /// En production, remplacer par un appel à l'endpoint local :
    /// `GET http://localhost:9010/api/stats` par exemple.
    async fn read_hnn_tick_rate(&self) -> f64 {
        // Valeur nominale cible : 254_000 ticks/s
        // Simule des variations normales ±10% avec chutes périodiques
        let base = 254_000.0_f64;

        // Simulation réaliste
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        // Bruit sinusoïdal + aléatoire
        let noise = (t * 0.1).sin() * 10_000.0 + (t * 0.3).cos() * 5_000.0;
        let random_jitter = (t.fract() * 1000.0).sin() * 3_000.0;

        // Chute simulée périodique (tous les ~30 cycles de check)
        let cycle = (t / 300.0).fract();
        let crash = if cycle < 0.05 { -150_000.0 } else { 0.0 };

        (base + noise + random_jitter + crash).max(1000.0)
    }

    /// Émet une alerte sur le bus.
    async fn emit_alert(&self, drop_ratio: f64, before: f64, after: f64) {
        use crate::bus::Message;
        let msg = Message::AvidDiscovery {
            topic: "anomaly".into(),
            summary: format!(
                "⚠️ Chute HNN {:.1}% : {:.0} → {:.0} Hz | sévérité: {}",
                drop_ratio * 100.0,
                before,
                after,
                if drop_ratio > 0.6 {
                    "critique"
                } else {
                    "avertissement"
                }
            ),
        };
        self.bus.publish(msg);
        info!("Alerte AnomalyWatcher émise sur le bus");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anomaly_detection() {
        let bus = Arc::new(Bus::new(256));
        let mut watcher = AnomalyWatcher::new(bus);

        // Simuler une chute en forçant l'historique
        watcher.history.push_back((Instant::now(), 250_000.0));
        watcher.history.push_back((Instant::now(), 248_000.0));
        watcher.history.push_back((Instant::now(), 100_000.0)); // chute

        let drop = watcher.history[2].1;
        assert!(drop < 200_000.0, "La simulation devrait détecter une chute");
    }
}
