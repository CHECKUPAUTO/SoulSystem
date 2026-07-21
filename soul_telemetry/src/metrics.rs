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

/// Snapshot non-atomique des métriques d'un cœur (pour export / debug).
pub struct CoreMetricsSnapshot {
    pub core_id: usize,
    pub tasks_executed: u64,
    pub tasks_stolen: u64,
    pub total_cycles: u64,
    pub thermal_backoff_events: usize,
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
        metrics
            .total_cycles
            .fetch_add(cycles_spent, Ordering::Relaxed);
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

    /// Agrège les métriques de TOUS les cœurs en un snapshot unique.
    /// Utilisé par `soul_forge` pour calculer le fitness global du scheduler.
    /// Coût : O(N) atomiques Relaxed (aucun verrou).
    pub fn aggregate_metrics(&self) -> (u64, u64) {
        let mut total_tasks: u64 = 0;
        let mut total_cycles: u64 = 0;
        for core in &self.cores_data {
            total_tasks += core.tasks_executed.load(Ordering::Relaxed);
            total_cycles += core.total_cycles.load(Ordering::Relaxed);
        }
        (total_tasks, total_cycles)
    }

    /// Snapshot des métriques de tous les cœurs pour l'export Prometheus.
    pub fn get_core_metrics(&self) -> Vec<CoreMetricsSnapshot> {
        self.cores_data
            .iter()
            .enumerate()
            .map(|(id, core)| CoreMetricsSnapshot {
                core_id: id,
                tasks_executed: core.tasks_executed.load(Ordering::Relaxed),
                tasks_stolen: core.tasks_stolen.load(Ordering::Relaxed),
                total_cycles: core.total_cycles.load(Ordering::Relaxed),
                thermal_backoff_events: core.thermal_backoff_events.load(Ordering::Relaxed),
            })
            .collect()
    }

    /// Demarre l'echantillonneur thermique : lit le capteur sysfs toutes les
    /// `period` et publie la valeur. Le thread ne detient qu'un `Weak` sur le
    /// hub -> il s'arrete de lui-meme des que le dernier `Arc<TelemetryHub>` est
    /// libere (aucun thread fantome, propre en test et a l'arret).
    pub fn spawn_thermal_sampler(
        self: &Arc<Self>,
        period: Duration,
    ) -> std::io::Result<JoinHandle<()>> {
        self.spawn_thermal_sampler_with_reader(period, read_thermal_millicelsius)
    }

    /// Variante injectable de l'echantillonneur, utilisee par les tests
    /// pour fournir des echantillons deterministes sans materiel reel.
    fn spawn_thermal_sampler_with_reader<F>(
        self: &Arc<Self>,
        period: Duration,
        reader: F,
    ) -> std::io::Result<JoinHandle<()>>
    where
        F: Fn() -> Option<u32> + Send + 'static,
    {
        let weak: Weak<Self> = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("thermal-sampler".to_string())
            .spawn(move || {
                while let Some(hub) = weak.upgrade() {
                    if let Some(milli) = reader() {
                        hub.thermal_millicelsius.store(milli, Ordering::Relaxed);
                    }
                    std::thread::sleep(period);
                }
            })
    }
}

/// Lit la temperature brute (millidegres) depuis un chemin donne.
/// Utilisee par la production sur sysfs et par les tests sur des fichiers temp.
fn read_thermal_millicelsius_from(path: &std::path::Path) -> Option<u32> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.trim().parse::<u32>().ok()
}

/// Lit la temperature brute (millidegres) depuis sysfs. HORS chemin chaud.
fn read_thermal_millicelsius() -> Option<u32> {
    read_thermal_millicelsius_from(std::path::Path::new(THERMAL_SYSFS_PATH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    // ------------------------------------------------------------------
    // 6.1  Valid sysfs parsing
    // ------------------------------------------------------------------
    #[test]
    fn lit_un_fichier_sysfs_valide() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("temp");
        std::fs::write(&path, b"42000").unwrap();
        assert_eq!(read_thermal_millicelsius_from(&path), Some(42_000));
    }

    #[test]
    fn ignore_le_retour_a_la_ligne() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("temp");
        std::fs::write(&path, b"42000\n").unwrap();
        assert_eq!(read_thermal_millicelsius_from(&path), Some(42_000));
    }

    // ------------------------------------------------------------------
    // 6.2  Invalid sysfs content
    // ------------------------------------------------------------------
    #[test]
    fn fichier_vide_retourne_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("temp");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(read_thermal_millicelsius_from(&path), None);
    }

    #[test]
    fn contenu_non_numerique_retourne_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("temp");
        std::fs::write(&path, b"pas un nombre").unwrap();
        assert_eq!(read_thermal_millicelsius_from(&path), None);
    }

    #[test]
    fn valeur_negative_retourne_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("temp");
        std::fs::write(&path, b"-42").unwrap();
        assert_eq!(read_thermal_millicelsius_from(&path), None);
    }

    #[test]
    fn valeur_hors_u32_retourne_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("temp");
        std::fs::write(&path, b"999999999999").unwrap();
        assert_eq!(read_thermal_millicelsius_from(&path), None);
    }

    // ------------------------------------------------------------------
    // 6.3  Missing sysfs file
    // ------------------------------------------------------------------
    #[test]
    fn chemin_inexistant_retourne_none() {
        let path = std::path::Path::new("/tmp/ce_chemin_n_existe_pas_sur_aucune_machine");
        assert_eq!(read_thermal_millicelsius_from(path), None);
    }

    // ------------------------------------------------------------------
    // 6.4  Deterministic sampler publication
    // ------------------------------------------------------------------
    #[test]
    fn echantillonneur_deterministe_publie_la_temperature() {
        let hub = Arc::new(TelemetryHub::new(4));
        let _h = hub
            .spawn_thermal_sampler_with_reader(Duration::from_millis(10), || Some(42_000))
            .expect("spawn");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if hub.current_temp_celsius() == 42 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "timeout en attendant l'echantillon thermique"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    // ------------------------------------------------------------------
    // 6.5  Fail-open behavior
    // ------------------------------------------------------------------
    #[test]
    fn echantillonneur_fail_open_sans_capteur() {
        let hub = Arc::new(TelemetryHub::new(4));
        // Avant tout echantillon : fail-open (pas de throttle), temp 0.
        assert!(!hub.check_thermal_status(0));
        assert_eq!(hub.current_temp_celsius(), 0);

        let _h = hub
            .spawn_thermal_sampler_with_reader(Duration::from_millis(10), || None)
            .expect("spawn");
        std::thread::sleep(Duration::from_millis(100));

        // Toujours fail-open puisque le reader injecte None
        assert!(!hub.check_thermal_status(0));
        assert_eq!(hub.current_temp_celsius(), 0);
    }

    // ------------------------------------------------------------------
    // 6.6  Thermal threshold behavior
    // ------------------------------------------------------------------
    #[test]
    fn temperature_sous_le_seuil_ne_declenche_pas_de_throttling() {
        let hub = Arc::new(TelemetryHub::new(4));
        // 40 °C < THERMAL_LIMIT_CELSIUS (80)
        hub.thermal_millicelsius.store(40_000, Ordering::Relaxed);
        assert!(!hub.check_thermal_status(0));
        assert!(!hub.check_thermal_status(1));
        assert!(!hub.check_thermal_status(2));
        assert!(!hub.check_thermal_status(3));
    }

    #[test]
    fn temperature_au_dessus_du_seuil_declenche_le_throttling() {
        let hub = Arc::new(TelemetryHub::new(4));
        // 90 °C > THERMAL_LIMIT_CELSIUS (80)
        hub.thermal_millicelsius.store(90_000, Ordering::Relaxed);
        assert!(hub.check_thermal_status(0));
        assert!(hub.check_thermal_status(1));
        let snap = hub.get_core_metrics();
        assert_eq!(snap[0].thermal_backoff_events, 1);
        assert_eq!(snap[1].thermal_backoff_events, 1);
    }

    #[test]
    fn temperature_a_la_limite_exacte_ne_declenche_pas() {
        let hub = Arc::new(TelemetryHub::new(4));
        // 80 °C === THERMAL_LIMIT_CELSIUS (80), pas > 80 => pas de throttling
        hub.thermal_millicelsius.store(80_000, Ordering::Relaxed);
        assert!(!hub.check_thermal_status(0));
    }

    #[test]
    fn temperature_zero_vaut_fail_open() {
        let hub = Arc::new(TelemetryHub::new(4));
        hub.thermal_millicelsius.store(0, Ordering::Relaxed);
        assert!(!hub.check_thermal_status(0));
    }

    // ------------------------------------------------------------------
    // 6.7  Thread shutdown
    // ------------------------------------------------------------------
    #[test]
    fn sampler_s_eteint_a_la_liberation_du_hub() {
        let hub = Arc::new(TelemetryHub::new(1));
        let h = hub
            .spawn_thermal_sampler_with_reader(Duration::from_millis(20), || Some(42_000))
            .expect("spawn");
        drop(hub);
        let start = Instant::now();
        h.join().expect("join sampler");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "le sampler n'a pas termine apres liberation du hub"
        );
    }

    // ------------------------------------------------------------------
    // 7.  Optional real-hardware smoke test (manual only)
    // ------------------------------------------------------------------
    #[test]
    #[ignore = "requires a Linux host exposing a real sysfs thermal sensor"]
    fn sampler_lit_le_capteur_reel_quand_disponible() {
        if !std::path::Path::new(THERMAL_SYSFS_PATH).exists() {
            panic!("sysfs {} absent", THERMAL_SYSFS_PATH);
        }
        let hub = Arc::new(TelemetryHub::new(4));
        assert!(!hub.check_thermal_status(0));
        assert_eq!(hub.current_temp_celsius(), 0);

        let _h = hub
            .spawn_thermal_sampler(Duration::from_millis(50))
            .expect("spawn thermal-sampler");

        std::thread::sleep(Duration::from_millis(300));

        let c = hub.current_temp_celsius();
        assert!(c > 0, "temperature non echantillonnee depuis sysfs : {}", c);
        assert!(c < 150, "temperature aberrante : {}", c);
        println!(
            "PREUVE thermique : capteur lu hors chemin chaud -> {} deg C",
            c
        );
    }
}
