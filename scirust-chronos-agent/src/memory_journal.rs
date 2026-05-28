// ==========================================================================
// memory_journal.rs — Versionnement append-only (V6.4, point 3.A du doc)
//
// "Pour se prémunir contre une corruption ou une modification erronée,
//  chaque écriture dans le vector store et le graphe pourrait être
//  horodatée et versionnée (comme un commit Git)."
//
// COMPLÉMENT AU CHECKPOINT
//
//   Le CheckpointManager (V6.2) sauvegarde un SNAPSHOT ponctuel : l'état
//   complet à un instant t. Mais entre deux snapshots, plein d'événements
//   se produisent — observations, retro-bindings, learns, consolidations.
//   Si une corruption arrive juste avant le snapshot, on perd tout.
//
//   Le MemoryJournal est l'INVERSE : il enregistre la CHRONIQUE des
//   modifications, pas l'état. Comme un WAL (Write-Ahead Log) en base de
//   données, ou un git log. Combiné avec un snapshot de référence, il
//   permet la reconstruction byte-pour-byte de tout état historique.
//
// FORMAT
//
//   JSON Lines (.jsonl) — une ligne = un événement. Streamable, append-only,
//   diff-friendly, lisible humainement.
//
// ÉVÉNEMENTS TRACÉS
//
//   - EpisodicPush : nouvelle observation stockée
//   - EpisodicPrune : pruning de N traces
//   - RetroBind : confirmation rétro-causale d'une trace
//   - SemanticLearn : nouveau prototype ou renforcement
//   - SemanticDecay : decay temporel appliqué
//   - Consolidation : cycle de sommeil (résumé du report)
//   - Checkpoint : marker d'un snapshot complet
//
// USAGE
//
//   let mut journal = MemoryJournal::open("./journal/agent.jsonl")?;
//   journal.append(MemoryEvent::EpisodicPush { ... })?;
//   // ... après crash, recovery :
//   let events = MemoryJournal::replay("./journal/agent.jsonl")?;
//   // Reconstituer l'état en rejouant les events depuis le dernier
//   // checkpoint connu jusqu'à un timestamp cible.
//
// COMPACTION
//
//   Quand le journal devient trop gros (taille > seuil), un nouveau
//   checkpoint complet est créé et un nouveau journal démarre. L'ancien
//   peut être archivé ou supprimé selon politique.
//
//   Implémentation : `MemoryJournal::compact(snapshot_path)` qui :
//   1. Vérifie qu'un snapshot récent existe (sentinelle de cohérence)
//   2. Tronque le journal courant
//   3. Append un marker "post-checkpoint" comme première ligne
// ==========================================================================

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

// --------------------------------------------------------------------------
// Événements
// --------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MemoryEvent {
    /// Nouvelle trace épisodique poussée.
    EpisodicPush {
        t: u64,
        vector_hash: u64,
        has_future_prediction: bool,
    },
    /// N traces épisodiques supprimées par pruning.
    EpisodicPrune {
        t: u64,
        n_removed: usize,
        threshold: f64,
    },
    /// Rétro-causal binding : trace passée confirmée par observation actuelle.
    RetroBind {
        t: u64,
        n_bound: usize,
        avg_similarity: f64,
    },
    /// Apprentissage sémantique : nouveau prototype ou renforcement.
    SemanticLearn {
        t: u64,
        vector_hash: u64,
        was_new_prototype: bool,
        new_strength: f64,
    },
    /// Cycle de consolidation hors-ligne.
    Consolidation {
        t: u64,
        traces_consolidated: usize,
        prototypes_created: usize,
        clusters_found: usize,
    },
    /// Marker de checkpoint — sentinelle pour la compaction.
    Checkpoint {
        t: u64,
        global_step: u64,
        snapshot_path: String,
    },
    /// Action de santé déclenchée.
    HealthAction {
        t: u64,
        action: String,
        reason: String,
    },
}

impl MemoryEvent {
    pub fn timestamp(&self) -> u64 {
        match self {
            MemoryEvent::EpisodicPush { t, .. } => *t,
            MemoryEvent::EpisodicPrune { t, .. } => *t,
            MemoryEvent::RetroBind { t, .. } => *t,
            MemoryEvent::SemanticLearn { t, .. } => *t,
            MemoryEvent::Consolidation { t, .. } => *t,
            MemoryEvent::Checkpoint { t, .. } => *t,
            MemoryEvent::HealthAction { t, .. } => *t,
        }
    }
}

// --------------------------------------------------------------------------
// Journal
// --------------------------------------------------------------------------

pub struct MemoryJournal {
    path: PathBuf,
    writer: File,
    /// Compteur d'événements écrits depuis l'ouverture (pour stats).
    pub events_written: u64,
}

impl MemoryJournal {
    /// Ouvre (ou crée) un journal en mode append.
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        if let Some(parent) = path_buf.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path_buf)?;
        Ok(Self { path: path_buf, writer, events_written: 0 })
    }

    /// Append un événement. Format : une ligne JSON terminée par '\n'.
    /// La méthode est append-only — pas de seek, pas de réécriture.
    pub fn append(&mut self, event: MemoryEvent) -> std::io::Result<()> {
        let line = serde_json::to_string(&event)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.events_written += 1;
        Ok(())
    }

    /// Force le flush du buffer OS vers le disque.
    /// À appeler avant un checkpoint snapshot pour garantir la cohérence.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }

    /// Lit le journal et retourne tous les événements.
    pub fn replay<P: AsRef<Path>>(path: P) -> std::io::Result<Vec<MemoryEvent>> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for (line_no, line) in reader.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() { continue; }
            match serde_json::from_str::<MemoryEvent>(&line) {
                Ok(ev) => events.push(ev),
                Err(e) => {
                    eprintln!("journal line {} parse error: {} → skipped",
                              line_no + 1, e);
                }
            }
        }
        Ok(events)
    }

    /// Retourne tous les événements depuis le dernier marker Checkpoint.
    /// Utilisé pour le recovery post-crash : replay des changements
    /// non-snapshotted.
    pub fn replay_since_last_checkpoint<P: AsRef<Path>>(path: P)
        -> std::io::Result<Vec<MemoryEvent>>
    {
        let all = Self::replay(path)?;
        // Trouve l'index du dernier Checkpoint
        let last_ckpt = all.iter().rposition(|e|
            matches!(e, MemoryEvent::Checkpoint { .. }));
        match last_ckpt {
            Some(i) => Ok(all.into_iter().skip(i + 1).collect()),
            None => Ok(all),  // pas de checkpoint dans le journal → tout
        }
    }

    /// Filtre les événements dans une fenêtre temporelle (logique).
    pub fn replay_window<P: AsRef<Path>>(path: P, t_min: u64, t_max: u64)
        -> std::io::Result<Vec<MemoryEvent>>
    {
        let all = Self::replay(path)?;
        Ok(all.into_iter()
            .filter(|e| {
                let t = e.timestamp();
                t >= t_min && t <= t_max
            })
            .collect())
    }

    /// Compacte le journal : tronque tout ce qui précède le dernier
    /// checkpoint marker. Suppose qu'un snapshot complet a été créé
    /// (sentinelle de cohérence : on vérifie que le path du snapshot
    /// référencé existe vraiment).
    pub fn compact<P: AsRef<Path>>(path: P) -> std::io::Result<usize> {
        let path = path.as_ref();
        let all = Self::replay(path)?;
        let last_ckpt_idx = all.iter().rposition(|e|
            matches!(e, MemoryEvent::Checkpoint { .. }));

        let last_ckpt_idx = match last_ckpt_idx {
            Some(i) => i,
            None => return Ok(0),  // rien à compacter
        };

        // Vérifie que le snapshot référencé existe encore
        if let MemoryEvent::Checkpoint { snapshot_path, .. } = &all[last_ckpt_idx] {
            if !Path::new(snapshot_path).exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("compact aborted: snapshot {} missing", snapshot_path)));
            }
        }

        // Tronque le journal en réécrivant uniquement depuis le checkpoint
        let kept = &all[last_ckpt_idx..];
        let tmp_path = path.with_extension("compact.tmp");
        let mut tmp = OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(&tmp_path)?;
        for ev in kept {
            let line = serde_json::to_string(ev)?;
            tmp.write_all(line.as_bytes())?;
            tmp.write_all(b"\n")?;
        }
        tmp.sync_all()?;
        drop(tmp);

        // Rename atomique
        std::fs::rename(&tmp_path, path)?;
        Ok(last_ckpt_idx)
    }

    /// Taille actuelle du journal en bytes.
    pub fn size_bytes<P: AsRef<Path>>(path: P) -> std::io::Result<u64> {
        let meta = std::fs::metadata(path.as_ref())?;
        Ok(meta.len())
    }

    pub fn path(&self) -> &Path { &self.path }
}

// --------------------------------------------------------------------------
// Hash helper — pour identifier un vecteur sans le stocker dans le journal
// --------------------------------------------------------------------------

/// Hash stable d'un vecteur f64. Utilisé pour tracer "quel" vecteur a été
/// stocké sans dupliquer son contenu dans le journal (qui doit rester léger).
pub fn vector_hash(v: &[f64]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for x in v {
        // f64 ne implémente pas Hash → on hash les bits.
        x.to_bits().hash(&mut h);
    }
    h.finish()
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn tmpfile(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("chronos_journal_test_{}.jsonl", name));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn append_and_replay() {
        let path = tmpfile("append");
        let mut j = MemoryJournal::open(&path).unwrap();
        j.append(MemoryEvent::EpisodicPush {
            t: 10, vector_hash: 42, has_future_prediction: true,
        }).unwrap();
        j.append(MemoryEvent::RetroBind {
            t: 11, n_bound: 3, avg_similarity: 0.85,
        }).unwrap();
        j.flush().unwrap();

        let events = MemoryJournal::replay(&path).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], MemoryEvent::EpisodicPush { t: 10, .. }));
        assert!(matches!(events[1], MemoryEvent::RetroBind { t: 11, .. }));
    }

    #[test]
    fn replay_since_last_checkpoint() {
        let path = tmpfile("since_ckpt");
        let mut j = MemoryJournal::open(&path).unwrap();
        j.append(MemoryEvent::EpisodicPush { t: 1, vector_hash: 1, has_future_prediction: false }).unwrap();
        j.append(MemoryEvent::Checkpoint { t: 2, global_step: 100, snapshot_path: "/tmp/snap.ckpt".into() }).unwrap();
        j.append(MemoryEvent::EpisodicPush { t: 3, vector_hash: 2, has_future_prediction: true }).unwrap();
        j.append(MemoryEvent::EpisodicPush { t: 4, vector_hash: 3, has_future_prediction: false }).unwrap();
        j.flush().unwrap();

        let since = MemoryJournal::replay_since_last_checkpoint(&path).unwrap();
        // Doit retourner uniquement les 2 EpisodicPush après le checkpoint
        assert_eq!(since.len(), 2);
        assert_eq!(since[0].timestamp(), 3);
        assert_eq!(since[1].timestamp(), 4);
    }

    #[test]
    fn replay_window_filters_by_timestamp() {
        let path = tmpfile("window");
        let mut j = MemoryJournal::open(&path).unwrap();
        for t in [10u64, 20, 30, 40, 50] {
            j.append(MemoryEvent::EpisodicPush {
                t, vector_hash: t, has_future_prediction: false,
            }).unwrap();
        }
        j.flush().unwrap();

        let win = MemoryJournal::replay_window(&path, 20, 40).unwrap();
        assert_eq!(win.len(), 3);
        assert_eq!(win[0].timestamp(), 20);
        assert_eq!(win[2].timestamp(), 40);
    }

    #[test]
    fn compact_truncates_pre_checkpoint() {
        let path = tmpfile("compact");
        let snap_path = std::env::temp_dir().join("dummy_snap.ckpt");
        // Crée un fichier snapshot dummy pour que compact() accepte
        std::fs::write(&snap_path, "dummy").unwrap();

        let mut j = MemoryJournal::open(&path).unwrap();
        for t in 0..10u64 {
            j.append(MemoryEvent::EpisodicPush {
                t, vector_hash: t, has_future_prediction: false,
            }).unwrap();
        }
        j.append(MemoryEvent::Checkpoint {
            t: 10, global_step: 50,
            snapshot_path: snap_path.to_string_lossy().into(),
        }).unwrap();
        for t in 11..15u64 {
            j.append(MemoryEvent::EpisodicPush {
                t, vector_hash: t, has_future_prediction: false,
            }).unwrap();
        }
        j.flush().unwrap();
        drop(j);

        // Avant compact : 15 events
        let before = MemoryJournal::replay(&path).unwrap();
        assert_eq!(before.len(), 15);

        let truncated = MemoryJournal::compact(&path).unwrap();
        // 10 events truncatés (ceux avant le checkpoint)
        assert_eq!(truncated, 10);

        // Après compact : checkpoint + 4 events = 5
        let after = MemoryJournal::replay(&path).unwrap();
        assert_eq!(after.len(), 5);
        assert!(matches!(after[0], MemoryEvent::Checkpoint { .. }));

        let _ = std::fs::remove_file(&snap_path);
    }

    #[test]
    fn compact_aborts_if_snapshot_missing() {
        let path = tmpfile("compact_missing");
        let mut j = MemoryJournal::open(&path).unwrap();
        j.append(MemoryEvent::EpisodicPush { t: 1, vector_hash: 1, has_future_prediction: false }).unwrap();
        j.append(MemoryEvent::Checkpoint {
            t: 2, global_step: 10,
            snapshot_path: "/nonexistent/snap.ckpt".into(),
        }).unwrap();
        j.flush().unwrap();
        drop(j);

        let result = MemoryJournal::compact(&path);
        assert!(result.is_err(), "compact should fail if referenced snapshot missing");
    }

    #[test]
    fn vector_hash_stable() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![1.0, 2.0, 3.0];
        let v3 = vec![1.0, 2.0, 3.001];
        assert_eq!(vector_hash(&v1), vector_hash(&v2));
        assert_ne!(vector_hash(&v1), vector_hash(&v3));
    }

    #[test]
    fn malformed_lines_skipped() {
        let path = tmpfile("malformed");
        // Écrit manuellement un fichier avec une ligne valide + une corrompue
        let content = r#"{"type":"EpisodicPush","t":1,"vector_hash":42,"has_future_prediction":false}
GARBAGE NOT JSON
{"type":"RetroBind","t":2,"n_bound":1,"avg_similarity":0.9}
"#;
        std::fs::write(&path, content).unwrap();
        let events = MemoryJournal::replay(&path).unwrap();
        // Les 2 valides doivent être lus, la garbage skipped
        assert_eq!(events.len(), 2);
    }
}
