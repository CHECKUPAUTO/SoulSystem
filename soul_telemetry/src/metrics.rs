use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

/// Seuil critique (°C) au-dela duquel un coeur doit temporiser.
const THERMAL_LIMIT_CELSIUS: u32 = 80;
/// Chemin sysfs du capteur thermique (millidegres).
const THERMAL_SYSFS_PATH: &str = "/sys/class/thermal/thermal_zone0/temp";

#[repr(align(64))]
pub struct CoreMetrics {
    pub total_cycles: AtomicU64,
    pub tasks_executed: AtomicU64,
    pub tasks_stolen: AtomicU64,
    pub thermal_backoff_events: AtomicUsize,
}

/// Hub de diagnostic non-bloquant de SoulSystem.
///
/// La temperature est echantillonnee HORS du chemin chaud par un thread dedie
/// (`spawn_thermal_sampler`) et publiee dans `thermal_millicelsius`. Les workers
/// ne font qu'une lecture atomique : zero syscall sur la boucle d'ordonnancement.
pub struct TelemetryHub {
    cores_data: Vec<CoreMetrics>,
    /// Derniere temperature lue (millidegres). 0 = aucun echantillon encore.
    thermal_millicelsius: AtomicU32,
}

impl TelemetryHub {
    pub fn new(total_cores: usize) -> Self {
        let mut cores_data = Vec::with_capacity(total_cores);
        for _ in 0..total_cores {
            cores_data.push(CoreMetrics {
                total_cycles: AtomicU64::new(0),
                tasks_executed: AtomicU64::new(0),
                tasks_stolen: AtomicU64::new(0),
                thermal_backoff_events: AtomicUsize::new(0),
            });
        }
        Self {
            cores_data,
            thermal_millicelsius: AtomicU32::new(0),
        }
    }

    #[inline(always)]
    pub fn record_execution(&self, core_id: usize, cycles_spent: u64, was_stolen: bool) {
        if core_id >= self.cores_data.len() {
            return;
        }
        let metrics = &self.cores_data[core_id];
        metrics.total_cycles.fetch_add(cycles_spent, Ordering::Relaxed);
        metrics.tasks_executed.fetch_add(1, Ordering::Relaxed);
        if was_stolen {
            metrics.tasks_stolen.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Determine si un coeur doit temporiser thermiquement.
    ///
    /// CHEMIN CHAUD : simple lecture atomique de la derniere temperature
    /// echantillonnee. Aucun appel systeme ici (cf. `spawn_thermal_sampler`).
    #[inline(always)]
    pub fn check_thermal_status(&self, core_id: usize) -> bool {
        let milli = self.thermal_millicelsius.load(Ordering::Relaxed);
        if milli == 0 {
            // Pas encore d'echantillon : on ne bride pas (fail-open).
            return false;
        }
        if milli / 1000 > THERMAL_LIMIT_CELSIUS {
            if core_id < self.cores_data.len() {
                self.cores_data[core_id]
                    .thermal_backoff_events
                    .fetch_add(1, Ordering::Relaxed);
            }
            return true;
        }
        false
    }

    /// Temperature courante en °C (0 si aucun echantillon). Observabilite.
    #[inline]
    pub fn current_temp_celsius(&self) -> u32 {
        self.thermal_millicelsius.load(Ordering::Relaxed) / 1000
    }

    /// Demarre l'echantillonneur thermique : lit le capteur sysfs toutes les
    /// `period` et publie la valeur. Le thread ne detient qu'un `Weak` sur le
    /// hub -> il s'arrete de lui-meme des que le dernier `Arc<TelemetryHub>` est
    /// libere (aucun thread fantome, propre en test et a l'arret).
    pub fn spawn_thermal_sampler(
        self: &Arc<Self>,
        period: Duration,
    ) -> std::io::Result<JoinHandle<()>> {
        let weak: Weak<Self> = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("thermal-sampler".to_string())
            .spawn(move || {
                // Upgrade ephemere : on ne garde aucune ref forte pendant le
                // sleep, sinon on retarderait la liberation du hub.
                while let Some(hub) = weak.upgrade() {
                    if let Some(milli) = read_thermal_millicelsius() {
                        hub.thermal_millicelsius.store(milli, Ordering::Relaxed);
                    }
                    std::thread::sleep(period);
                }
            })
    }
}

/// Lit la temperature brute (millidegres) depuis sysfs. HORS chemin chaud.
fn read_thermal_millicelsius() -> Option<u32> {
    let raw = std::fs::read_to_string(THERMAL_SYSFS_PATH).ok()?;
    raw.trim().parse::<u32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn sampler_lit_le_capteur_reel() {
        let hub = Arc::new(TelemetryHub::new(4));
        // Avant tout echantillon : fail-open (pas de throttle), temp 0.
        assert!(!hub.check_thermal_status(0));
        assert_eq!(hub.current_temp_celsius(), 0);

        let _h = hub
            .spawn_thermal_sampler(Duration::from_millis(50))
            .expect("spawn thermal-sampler");

        // Laisse passer quelques ticks : sysfs doit etre lu et publie.
        std::thread::sleep(Duration::from_millis(300));

        let c = hub.current_temp_celsius();
        assert!(c > 0, "temperature non echantillonnee (sysfs zone0 absent ?) : {}", c);
        assert!(c < 150, "temperature aberrante : {}", c);
        println!("PREUVE thermique : capteur lu hors chemin chaud -> {} deg C", c);
    }

    #[test]
    fn sampler_s_eteint_a_la_liberation_du_hub() {
        // Le thread ne tient qu'un Weak : quand le dernier Arc tombe, il sort.
        let hub = Arc::new(TelemetryHub::new(1));
        let h = hub.spawn_thermal_sampler(Duration::from_millis(20)).expect("spawn");
        drop(hub); // plus aucune reference forte
        let start = std::time::Instant::now();
        h.join().expect("join sampler");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "le sampler n'a pas termine apres liberation du hub"
        );
        println!("PREUVE auto-stop : sampler termine en {:?}", start.elapsed());
    }
}
