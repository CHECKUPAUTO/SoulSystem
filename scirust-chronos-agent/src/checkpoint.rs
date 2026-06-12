// ==========================================================================
// checkpoint.rs — Persistance des poids appris + état mémoire (V6.2)
//
// Comble le trou critique identifié dans le schéma : aucun lien entre le
// ChronosAgent (noyau latent en RAM) et le SoulSystem (mémoire persistante).
// Sans ce pont, la convergence mesurée (-30% coherence, +43% α_sync) s'évapore
// à chaque redémarrage du process.
//
// ARCHITECTURE
//
//   1. POIDS APPRIS via safetensors (format binaire standard, mmap-able)
//      - ScoreNetwork : w1, w2, w3, b1, b2, b3
//      - PotentialMLP : w1, w2, b1, b2
//      Format clé : "score_net/w1", "potential/w2", etc.
//
//   2. ÉTAT MÉMOIRE via JSON (lisible, debug-friendly, taille modeste)
//      - ReplayBuffer (latents + conditions + quality + métadonnées)
//      - AtemporalMemory (épisodique + sémantique + alpha_sync)
//      - Compteur de step global pour reprendre l'entraînement
//
//   3. MANIFEST (JSON) qui décrit le bundle : version, timestamps, configs,
//      hash de validation.
//
//   Layout d'un checkpoint :
//
//     <ckpt_dir>/
//       manifest.json
//       weights.safetensors        ← poids tous composants
//       replay.json                ← ReplayBuffer
//       atemporal_memory.json      ← MAA dump
//
// USAGE main.rs
//
//   let ckpt = CheckpointManager::new("./checkpoints/agent")?;
//
//   // au démarrage
//   if ckpt.exists() {
//       ckpt.load(&mut planner.score_net, &mut bci.potential,
//                &mut replay, &mut memory, &mut global_step)?;
//   }
//
//   // tous les N steps
//   if step % CHECKPOINT_EVERY == 0 {
//       ckpt.save(&planner.score_net, &bci.potential,
//                 &replay, &memory, global_step)?;
//   }
//
// SAFETY
//
//   - Écriture atomique : on écrit dans <name>.tmp puis rename pour éviter
//     les corruptions si crash en cours d'écriture.
//   - Validation au load : check de cohérence des dimensions vs config
//     courante. Si mismatch, refuse de charger (mieux qu'un crash silencieux).
//   - Version du format dans le manifest. Les futures versions feront
//     migration ou refus propre.
// ==========================================================================

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use candle_core::{Device, Result, Tensor, Var};
use serde::{Deserialize, Serialize};

use crate::planner::ScoreNetwork;
use crate::bci::PotentialMLP;
use crate::training::ReplayBuffer;
use crate::memory::{AtemporalMemory, VectorIndex};

// --------------------------------------------------------------------------
// Manifest
// --------------------------------------------------------------------------

pub const CHECKPOINT_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CheckpointManifest {
    pub format_version: u32,
    pub agent_version: String,
    /// Step global au moment du checkpoint.
    pub global_step: u64,
    /// Timestamp Unix (secondes) d'écriture.
    pub timestamp: i64,
    /// Configuration du ScoreNetwork.
    pub score_net_config: ScoreNetConfig,
    /// Configuration du PotentialMLP.
    pub potential_config: PotentialConfig,
    /// Taille du ReplayBuffer au moment du dump.
    pub replay_size: usize,
    /// Latents stockés en mémoire épisodique.
    pub episodic_size: usize,
    /// Prototypes sémantiques.
    pub semantic_size: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScoreNetConfig {
    pub d_latent: usize,
    pub d_t: usize,
    pub d_cond: usize,
    pub d_hidden: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PotentialConfig {
    pub d_q: usize,
    pub d_h: usize,
}

// --------------------------------------------------------------------------
// État mémoire sérialisable
// --------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct ReplayDump {
    capacity: usize,
    d_latent: usize,
    latents: Vec<Vec<f64>>,
    conditions: Vec<Vec<f64>>,
    quality: Vec<bool>,
}

#[derive(Serialize, Deserialize)]
struct EpisodicTraceDump {
    vector: Vec<f64>,
    t_stored: u64,
    future_prediction: Option<Vec<f64>>,
    last_confirmed: u64,
    confirmations: u32,
    /// V6.2 — fréquence d'accès. Default à 0 pour compat backward.
    #[serde(default)]
    access_count: u32,
}

#[derive(Serialize, Deserialize)]
struct SemanticPrototypeDump {
    vector: Vec<f64>,
    strength: f64,
    t_created: u64,
    t_last_reinforced: u64,
}

#[derive(Serialize, Deserialize)]
struct MemoryDump {
    current_t: u64,
    alpha_sync: f64,
    alpha_raw: f64,
    alpha_episodic: f64,
    alpha_predictive: f64,
    retro_bindings_total: u64,
    episodic_capacity: usize,
    episodic_dim: usize,
    episodic: Vec<EpisodicTraceDump>,
    semantic_dim: usize,
    semantic_lambda: f64,
    semantic_prototypes: Vec<SemanticPrototypeDump>,
}

// --------------------------------------------------------------------------
// CheckpointManager
// --------------------------------------------------------------------------

pub struct CheckpointManager {
    pub dir: PathBuf,
}

impl CheckpointManager {
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|e|
            candle_core::Error::Msg(format!("create checkpoint dir: {}", e)).bt())?;
        Ok(Self { dir })
    }

    pub fn exists(&self) -> bool {
        self.dir.join("manifest.json").exists()
            && self.dir.join("weights.safetensors").exists()
    }

    /// Sauvegarde atomique : écrit dans .tmp puis rename pour éviter la
    /// corruption en cas de crash.
    pub fn save(
        &self,
        score_net: &ScoreNetwork,
        potential: &PotentialMLP,
        replay: &ReplayBuffer,
        memory: &AtemporalMemory,
        global_step: u64,
    ) -> Result<()> {
        // ---- 1. Manifest ----
        let manifest = CheckpointManifest {
            format_version: CHECKPOINT_FORMAT_VERSION,
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            global_step,
            timestamp: chrono_now_unix(),
            score_net_config: ScoreNetConfig {
                d_latent: score_net.d_latent,
                d_t: score_net.d_t,
                d_cond: score_net.d_cond,
                d_hidden: score_net.d_hidden,
            },
            potential_config: PotentialConfig {
                d_q: potential.d_q,
                d_h: potential.d_h,
            },
            replay_size: replay.len(),
            episodic_size: memory.episodic.traces.len(),
            semantic_size: memory.semantic.prototypes.len(),
        };

        // ---- 2. Weights (safetensors) ----
        let mut tensors: HashMap<String, Tensor> = HashMap::new();
        tensors.insert("score_net/w1".into(), score_net.w1.as_tensor().clone());
        tensors.insert("score_net/w2".into(), score_net.w2.as_tensor().clone());
        tensors.insert("score_net/w3".into(), score_net.w3.as_tensor().clone());
        tensors.insert("score_net/b1".into(), score_net.b1.as_tensor().clone());
        tensors.insert("score_net/b2".into(), score_net.b2.as_tensor().clone());
        tensors.insert("score_net/b3".into(), score_net.b3.as_tensor().clone());
        tensors.insert("potential/w1".into(), potential.w1.as_tensor().clone());
        tensors.insert("potential/w2".into(), potential.w2.as_tensor().clone());
        tensors.insert("potential/b1".into(), potential.b1.as_tensor().clone());
        tensors.insert("potential/b2".into(), potential.b2.as_tensor().clone());

        let weights_tmp = self.dir.join("weights.safetensors.tmp");
        candle_core::safetensors::save(&tensors, &weights_tmp)?;

        // ---- 3. ReplayBuffer ----
        let replay_dump = ReplayDump {
            capacity: replay.capacity,
            d_latent: replay.d_latent,
            latents: replay.latents.clone(),
            conditions: replay.conditions.clone(),
            quality: replay.quality.clone(),
        };
        let replay_tmp = self.dir.join("replay.json.tmp");
        write_json(&replay_tmp, &replay_dump)?;

        // ---- 4. Memory ----
        let memory_dump = MemoryDump {
            current_t: memory.current_t,
            alpha_sync: memory.alpha_sync,
            alpha_raw: memory.alpha_raw,
            alpha_episodic: memory.alpha_episodic,
            alpha_predictive: memory.alpha_predictive,
            retro_bindings_total: memory.retro_bindings_total,
            episodic_capacity: memory.episodic.capacity,
            episodic_dim: memory.episodic.dim,
            episodic: memory.episodic.traces.iter().map(|t| EpisodicTraceDump {
                vector: t.vector.clone(),
                t_stored: t.t_stored,
                future_prediction: t.future_prediction.clone(),
                last_confirmed: t.last_confirmed,
                confirmations: t.confirmations,
                access_count: t.access_count,
            }).collect(),
            semantic_dim: memory.semantic.index.dim(),
            semantic_lambda: memory.semantic.lambda,
            semantic_prototypes: memory.semantic.prototypes.iter().map(|p| SemanticPrototypeDump {
                vector: p.vector.clone(),
                strength: p.strength,
                t_created: p.t_created,
                t_last_reinforced: p.t_last_reinforced,
            }).collect(),
        };
        let memory_tmp = self.dir.join("atemporal_memory.json.tmp");
        write_json(&memory_tmp, &memory_dump)?;

        // ---- 5. Manifest (en dernier — sentinelle d'intégrité) ----
        let manifest_tmp = self.dir.join("manifest.json.tmp");
        write_json(&manifest_tmp, &manifest)?;

        // ---- 6. Rename atomique de tous les fichiers ----
        // Ordre : weights, replay, memory, manifest (le manifest sert de
        // sentinelle — sa présence atteste de la complétude du bundle).
        rename(&weights_tmp, &self.dir.join("weights.safetensors"))?;
        rename(&replay_tmp, &self.dir.join("replay.json"))?;
        rename(&memory_tmp, &self.dir.join("atemporal_memory.json"))?;
        rename(&manifest_tmp, &self.dir.join("manifest.json"))?;

        Ok(())
    }

    /// Charge en mémoire — overwrite les poids des Var via `.set()` (préserve
    /// l'identité des Var, donc AdamW continue de tracker les mêmes paramètres).
    /// Retourne le global_step recovered.
    pub fn load(
        &self,
        score_net: &ScoreNetwork,
        potential: &PotentialMLP,
        replay: &mut ReplayBuffer,
        memory: &mut AtemporalMemory,
        device: &Device,
    ) -> Result<u64> {
        // ---- 1. Manifest + validation config ----
        let manifest: CheckpointManifest = read_json(&self.dir.join("manifest.json"))?;
        if manifest.format_version != CHECKPOINT_FORMAT_VERSION {
            return Err(candle_core::Error::Msg(format!(
                "checkpoint format version mismatch: file={} current={}",
                manifest.format_version, CHECKPOINT_FORMAT_VERSION)).bt());
        }
        // Dimensions doivent matcher la config courante (refuse silent reshape).
        if manifest.score_net_config.d_latent != score_net.d_latent
            || manifest.score_net_config.d_hidden != score_net.d_hidden
            || manifest.score_net_config.d_t != score_net.d_t
            || manifest.score_net_config.d_cond != score_net.d_cond
        {
            return Err(candle_core::Error::Msg(format!(
                "ScoreNetwork dims mismatch: file={:?} current=(d_lat={}, d_hid={}, d_t={}, d_cond={})",
                manifest.score_net_config,
                score_net.d_latent, score_net.d_hidden, score_net.d_t, score_net.d_cond
            )).bt());
        }
        if manifest.potential_config.d_q != potential.d_q
            || manifest.potential_config.d_h != potential.d_h
        {
            return Err(candle_core::Error::Msg(format!(
                "PotentialMLP dims mismatch: file=(d_q={}, d_h={}) current=(d_q={}, d_h={})",
                manifest.potential_config.d_q, manifest.potential_config.d_h,
                potential.d_q, potential.d_h
            )).bt());
        }

        // ---- 2. Weights ----
        let tensors = candle_core::safetensors::load(
            &self.dir.join("weights.safetensors"), device)?;
        set_var(&score_net.w1, &tensors, "score_net/w1")?;
        set_var(&score_net.w2, &tensors, "score_net/w2")?;
        set_var(&score_net.w3, &tensors, "score_net/w3")?;
        set_var(&score_net.b1, &tensors, "score_net/b1")?;
        set_var(&score_net.b2, &tensors, "score_net/b2")?;
        set_var(&score_net.b3, &tensors, "score_net/b3")?;
        set_var(&potential.w1, &tensors, "potential/w1")?;
        set_var(&potential.w2, &tensors, "potential/w2")?;
        set_var(&potential.b1, &tensors, "potential/b1")?;
        set_var(&potential.b2, &tensors, "potential/b2")?;

        // ---- 3. ReplayBuffer ----
        let dump: ReplayDump = read_json(&self.dir.join("replay.json"))?;
        replay.capacity = dump.capacity;
        replay.d_latent = dump.d_latent;
        replay.latents = dump.latents;
        replay.conditions = dump.conditions;
        replay.quality = dump.quality;

        // ---- 4. Memory ----
        let mem_dump: MemoryDump = read_json(&self.dir.join("atemporal_memory.json"))?;
        memory.current_t = mem_dump.current_t;
        memory.alpha_sync = mem_dump.alpha_sync;
        memory.alpha_raw = mem_dump.alpha_raw;
        memory.alpha_episodic = mem_dump.alpha_episodic;
        memory.alpha_predictive = mem_dump.alpha_predictive;
        memory.retro_bindings_total = mem_dump.retro_bindings_total;

        memory.episodic.traces.clear();
        for d in mem_dump.episodic {
            memory.episodic.traces.push_back(crate::memory::EpisodicTrace {
                vector: d.vector,
                t_stored: d.t_stored,
                future_prediction: d.future_prediction,
                last_confirmed: d.last_confirmed,
                confirmations: d.confirmations,
                access_count: d.access_count,
            });
        }
        // V6.3 — Reconstruit l'index temporel à partir des traces chargées.
        let entries: Vec<(u64, usize)> = memory.episodic.traces.iter().enumerate()
            .map(|(i, t)| (t.t_stored, i)).collect();
        memory.episodic.temporal_idx.rebuild_from(entries);

        // Sémantique : on doit reconstruire l'index HNSW (lazy rebuild)
        memory.semantic.lambda = mem_dump.semantic_lambda;
        memory.semantic.prototypes.clear();
        memory.semantic.index.vectors.clear();
        for p in mem_dump.semantic_prototypes {
            memory.semantic.index.insert(p.vector.clone());
            memory.semantic.prototypes.push(crate::memory::SemanticPrototype {
                vector: p.vector,
                strength: p.strength,
                t_created: p.t_created,
                t_last_reinforced: p.t_last_reinforced,
            });
        }
        memory.semantic.compact();  // force rebuild HNSW

        Ok(manifest.global_step)
    }

    pub fn last_manifest(&self) -> Result<CheckpointManifest> {
        read_json(&self.dir.join("manifest.json"))
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn set_var(var: &Var, tensors: &HashMap<String, Tensor>, key: &str) -> Result<()> {
    let t = tensors.get(key).ok_or_else(||
        candle_core::Error::Msg(format!("missing tensor: {}", key)).bt())?;
    var.set(t)?;
    Ok(())
}

fn write_json<P: AsRef<Path>, T: Serialize>(path: P, value: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(value).map_err(|e|
        candle_core::Error::Msg(format!("json serialize: {}", e)).bt())?;
    fs::write(path.as_ref(), s).map_err(|e|
        candle_core::Error::Msg(format!("write {}: {}", path.as_ref().display(), e)).bt())?;
    Ok(())
}

fn read_json<P: AsRef<Path>, T: for<'a> Deserialize<'a>>(path: P) -> Result<T> {
    let s = fs::read_to_string(path.as_ref()).map_err(|e|
        candle_core::Error::Msg(format!("read {}: {}", path.as_ref().display(), e)).bt())?;
    serde_json::from_str(&s).map_err(|e|
        candle_core::Error::Msg(format!("json parse {}: {}", path.as_ref().display(), e)).bt())
}

fn rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to).map_err(|e|
        candle_core::Error::Msg(format!("rename {} → {}: {}",
            from.display(), to.display(), e)).bt())
}

fn chrono_now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::AtemporalMemory;
    use crate::training::ReplayBuffer;

    fn dev() -> Device { Device::Cpu }

    fn tmpdir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("chronos_ckpt_test_{}", name));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn roundtrip_weights_preserves_values() {
        let dir = tmpdir("roundtrip_weights");
        let device = dev();

        let score_net = ScoreNetwork::new(16, 32, 16, 64, &device).unwrap();
        let potential = PotentialMLP::new(8, 32, &device).unwrap();
        let replay = ReplayBuffer::new(64, 16);
        let memory = AtemporalMemory::new(16, 32);

        // Snapshot AVANT save
        let w1_before: Vec<f64> = score_net.w1.as_tensor()
            .flatten_all().unwrap().to_vec1::<f64>().unwrap();

        // Save
        let ckpt = CheckpointManager::new(&dir).unwrap();
        ckpt.save(&score_net, &potential, &replay, &memory, 42).unwrap();
        assert!(ckpt.exists());

        // Modifie les poids in-place
        let zero = Tensor::zeros_like(score_net.w1.as_tensor()).unwrap();
        score_net.w1.set(&zero).unwrap();
        let w1_zeroed: Vec<f64> = score_net.w1.as_tensor()
            .flatten_all().unwrap().to_vec1::<f64>().unwrap();
        assert!(w1_zeroed.iter().all(|v| *v == 0.0));

        // Load → doit restaurer
        let mut replay_load = ReplayBuffer::new(64, 16);
        let mut memory_load = AtemporalMemory::new(16, 32);
        let step = ckpt.load(&score_net, &potential, &mut replay_load,
                             &mut memory_load, &device).unwrap();
        assert_eq!(step, 42);

        let w1_after: Vec<f64> = score_net.w1.as_tensor()
            .flatten_all().unwrap().to_vec1::<f64>().unwrap();
        for (a, b) in w1_before.iter().zip(w1_after.iter()) {
            assert!((a - b).abs() < 1e-12, "weight not restored: {} vs {}", a, b);
        }
    }

    #[test]
    fn roundtrip_replay_buffer() {
        let dir = tmpdir("roundtrip_replay");
        let device = dev();

        let score_net = ScoreNetwork::new(8, 16, 8, 32, &device).unwrap();
        let potential = PotentialMLP::new(4, 16, &device).unwrap();
        let mut replay = ReplayBuffer::new(32, 8);

        // Remplit le replay
        for i in 0..10 {
            replay.push(
                vec![i as f64; 8],
                vec![(i + 100) as f64; 8],
                i % 2 == 0,
            );
        }
        let memory = AtemporalMemory::new(8, 16);

        let ckpt = CheckpointManager::new(&dir).unwrap();
        ckpt.save(&score_net, &potential, &replay, &memory, 7).unwrap();

        let mut replay_load = ReplayBuffer::new(32, 8);
        let mut memory_load = AtemporalMemory::new(8, 16);
        ckpt.load(&score_net, &potential, &mut replay_load, &mut memory_load, &device).unwrap();

        assert_eq!(replay_load.len(), 10);
        assert_eq!(replay_load.latents[5][0], 5.0);
        assert_eq!(replay_load.conditions[5][0], 105.0);
        assert_eq!(replay_load.quality[5], false);  // 5 % 2 != 0
        assert_eq!(replay_load.quality[6], true);
    }

    #[test]
    fn roundtrip_memory_episodic_and_semantic() {
        let dir = tmpdir("roundtrip_memory");
        let device = dev();

        let score_net = ScoreNetwork::new(8, 16, 8, 32, &device).unwrap();
        let potential = PotentialMLP::new(4, 16, &device).unwrap();
        let replay = ReplayBuffer::new(32, 8);

        let mut memory = AtemporalMemory::new(8, 16);
        // Quelques observations pour peupler épisodique et sémantique
        for i in 0..5 {
            let mut v = vec![0.0; 8];
            v[i % 8] = 1.0;
            memory.observe(&v, Some(v.clone()));
        }
        let cur_t_before = memory.current_t;
        let episodic_before = memory.episodic.traces.len();
        let semantic_before = memory.semantic.prototypes.len();

        let ckpt = CheckpointManager::new(&dir).unwrap();
        ckpt.save(&score_net, &potential, &replay, &memory, 99).unwrap();

        let mut memory_load = AtemporalMemory::new(8, 16);
        let mut replay_load = ReplayBuffer::new(32, 8);
        ckpt.load(&score_net, &potential, &mut replay_load,
                  &mut memory_load, &device).unwrap();

        assert_eq!(memory_load.current_t, cur_t_before);
        assert_eq!(memory_load.episodic.traces.len(), episodic_before);
        assert_eq!(memory_load.semantic.prototypes.len(), semantic_before);
        // Vérifie qu'un recall fonctionne après reload (index HNSW reconstruit)
        let result = memory_load.retrieve(&vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 3);
        assert!(!result.is_empty(), "recall failed after reload");
    }

    #[test]
    fn dimension_mismatch_rejected() {
        let dir = tmpdir("dim_mismatch");
        let device = dev();

        let score_net = ScoreNetwork::new(16, 32, 16, 64, &device).unwrap();
        let potential = PotentialMLP::new(8, 32, &device).unwrap();
        let replay = ReplayBuffer::new(32, 16);
        let memory = AtemporalMemory::new(16, 16);

        let ckpt = CheckpointManager::new(&dir).unwrap();
        ckpt.save(&score_net, &potential, &replay, &memory, 1).unwrap();

        // Tente de charger dans des structures de mauvaise dim
        let wrong_score = ScoreNetwork::new(8, 32, 8, 32, &device).unwrap();  // d_lat=8 != 16
        let wrong_pot = PotentialMLP::new(8, 32, &device).unwrap();
        let mut r = ReplayBuffer::new(32, 8);
        let mut m = AtemporalMemory::new(8, 16);
        let result = ckpt.load(&wrong_score, &wrong_pot, &mut r, &mut m, &device);
        assert!(result.is_err(), "should reject dimension mismatch");
    }
}
