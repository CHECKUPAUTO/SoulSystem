//! # Persistent State Engine (CCOS v0.3)
//!
//! Durably stores the full runtime so CCOS can resume *exactly* where it left
//! off after a shutdown or reboot. State is written to a directory as three
//! files:
//!
//! - `graph.snapshot`  — the causal [`MemoryGraph`]
//! - `events.log`      — the append-only [`EventLog`]
//! - `memory.snapshot` — the hash-chained [`DistributedEventLog`]
//!
//! [`PersistentRuntime::restore_runtime`] reloads and *verifies* the state
//! (hash chain valid, no dangling edges) before handing it back.
//!
//! # Set-level integrity (INV-PERSIST-1)
//!
//! Each of the three files is written atomically — temp file, fsync, rename —
//! so no single file can be observed half-written. That is not the same as the
//! *set* being consistent. `save_state` performs three separate renames, and a
//! crash between them leaves a new `graph.snapshot` beside an old `events.log`:
//! every file individually intact, the set as a whole describing a moment that
//! never existed. Nothing detected that, because nothing recorded what the set
//! was supposed to look like.
//!
//! A fourth file, `state.manifest`, now records the format version, a
//! generation counter, and a SHA-256 digest of each member file. It is written
//! **last**, after the three renames, which is what makes a tear detectable:
//!
//! - Crash before any rename → the old set and the old manifest still agree.
//! - Crash between renames → the manifest still describes the *previous*
//!   generation, so its digests no longer match what is on disk. Detected.
//! - Crash after the last rename but before the manifest → same, detected.
//! - Manifest written → the set is complete and self-describing.
//!
//! This buys **detection, not atomicity**. There is no rollback: a torn set is
//! reported, not repaired. True multi-file atomicity needs either a single-file
//! format or a write-ahead log, and neither is a change to make quietly under a
//! security fix — see `docs/security/REMAINING_SECURITY_WORK.md`.

use crate::distributed_event_log::DistributedEventLog;
use crate::event_log::EventLog;
use crate::memory::MemoryGraph;
use crate::persist::KernelSnapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// On-disk format version for the three-file state directory.
///
/// Bumped when the layout changes in a way an older binary would misread. A
/// manifest declaring a *newer* version than this is refused rather than
/// parsed on a best-effort basis: silently reading a future layout is how a
/// state file becomes a corruption bug in a version you are no longer running.
pub const STATE_FORMAT_VERSION: u32 = 1;

/// File name of the manifest that describes the set.
pub const MANIFEST_FILE: &str = "state.manifest";

/// Subdirectory holding the previous verified generation.
///
/// P1-7 bought detection without recovery: a torn set was reported and refused,
/// which is the right failure mode but is still a failure — the process could
/// not start, and there was nowhere to fall back to. Retaining generation *n-1*
/// is what turns "refuses to load" into "loses the last save".
///
/// The cost is disk: the state directory roughly doubles. That is the trade
/// being made, and it is a cheap one against an unstartable process.
pub const PREVIOUS_DIR: &str = "state.prev";

/// What a manifest records about one saved generation of the state set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateManifest {
    /// Layout version — see [`STATE_FORMAT_VERSION`].
    pub format_version: u32,
    /// Incremented on every successful save, so two directories can be
    /// compared and an accidental restore of an older backup is visible.
    pub generation: u64,
    /// SHA-256 hex digest per member file, keyed by file name.
    pub files: BTreeMap<String, String>,
}

/// The outcome of checking a state directory against its manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetIntegrity {
    /// Manifest present and every digest matches.
    Verified {
        /// The generation the manifest describes.
        generation: u64,
    },
    /// No manifest. The directory predates manifests, so its consistency
    /// cannot be established either way.
    ///
    /// This is deliberately **not** an error, so that upgrading does not strand
    /// an existing state directory — the next `save_state` writes a manifest
    /// and the directory becomes verifiable. The cost is stated plainly: since
    /// a missing manifest is indistinguishable from a *deleted* one, anyone who
    /// can write to the state directory can downgrade it to this level. Callers
    /// that need to refuse that use [`PersistentRuntime::restore_runtime_strict`].
    Legacy,
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Typed error for persistence operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum PersistenceError {
    Io(String),
    Serde(String),
    Integrity(String),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Io(e) => write!(f, "persistence I/O error: {e}"),
            PersistenceError::Serde(e) => write!(f, "persistence (de)serialization error: {e}"),
            PersistenceError::Integrity(e) => write!(f, "persistence integrity error: {e}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<std::io::Error> for PersistenceError {
    fn from(e: std::io::Error) -> Self {
        PersistenceError::Io(e.to_string())
    }
}
impl From<serde_json::Error> for PersistenceError {
    fn from(e: serde_json::Error) -> Self {
        PersistenceError::Serde(e.to_string())
    }
}

/// The complete runtime state that survives a restart.
///
/// This is the **same payload**, field-for-field, as
/// [`crate::persist::KernelSnapshot`] (they were duplicated), so `RuntimeState`
/// is now a type alias for it: one state type, two on-disk layouts —
/// [`PersistentRuntime`] stores it as a three-file directory, while
/// [`KernelSnapshot::save`]/[`load`](KernelSnapshot::load) store it as a single
/// file. The integrity check is shared via [`KernelSnapshot::verify_integrity`].
pub type RuntimeState = KernelSnapshot;

/// Reads/writes [`RuntimeState`] under a directory.
#[derive(Debug, Clone)]
pub struct PersistentRuntime {
    pub dir: PathBuf,
}

impl PersistentRuntime {
    /// Use `dir` as the state directory (e.g. `"data"`).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn graph_path(&self) -> PathBuf {
        self.dir.join("graph.snapshot")
    }
    pub fn events_path(&self) -> PathBuf {
        self.dir.join("events.log")
    }
    pub fn memory_path(&self) -> PathBuf {
        self.dir.join("memory.snapshot")
    }

    /// True if a previously-saved state exists in the directory.
    pub fn exists(&self) -> bool {
        self.graph_path().exists() && self.events_path().exists() && self.memory_path().exists()
    }

    /// Persist `state` to the directory (creating it if needed).
    pub fn save_state(&self, state: &RuntimeState) -> Result<(), PersistenceError> {
        std::fs::create_dir_all(&self.dir)?;

        // Snapshot the outgoing generation *before* overwriting anything, so a
        // crash during the writes below has somewhere to fall back to.
        //
        // Only a **verified** set is snapshotted. Copying a set that is already
        // torn would replace a good fallback with a bad one, so a save that
        // finds the live set damaged leaves the existing snapshot alone — the
        // older good generation is worth more than the newer broken one. A
        // Legacy set is likewise not snapshotted: without a manifest there is
        // nothing to verify it against later, so the copy could not be trusted
        // to repair from.
        if matches!(
            self.verify_set_integrity(),
            Ok(SetIntegrity::Verified { .. })
        ) {
            // A failure to snapshot must not fail the save. The snapshot is a
            // recovery aid; refusing to persist new state because the *backup*
            // of the old state could not be written would trade a real loss for
            // a hypothetical one.
            if let Err(e) = self.snapshot_previous_generation() {
                eprintln!(
                    "[ccos] warning: could not retain previous generation for repair: {e}; \
                     continuing with the save"
                );
            }
        }

        // Each write is individually atomic; the manifest that follows is what
        // makes the *set* verifiable. Digests are taken from the exact bytes
        // written, not by re-reading the files afterwards — re-reading would
        // record whatever happened to be on disk at that moment, which is the
        // very thing the manifest is supposed to pin down.
        let mut files = BTreeMap::new();
        for (path, bytes) in [
            (self.graph_path(), serde_json::to_vec(&state.graph)?),
            (self.events_path(), serde_json::to_vec(&state.event_log)?),
            (self.memory_path(), serde_json::to_vec(&state.dist_log)?),
        ] {
            crate::util::write_durable(&path, &bytes)?;
            files.insert(file_name_of(&path)?, digest_hex(&bytes));
        }

        let generation = self.read_manifest().ok().flatten().map_or(0, |m| {
            // Saturating rather than wrapping: a generation that silently
            // returns to 0 would make an old backup look newer than the live
            // state, which is exactly the confusion the counter exists to
            // prevent.
            m.generation.saturating_add(1)
        });

        // Written last, and only after the three renames have landed. A crash
        // anywhere above leaves the previous manifest describing digests that
        // no longer match, which is what makes the tear detectable.
        self.write_manifest(&StateManifest {
            format_version: STATE_FORMAT_VERSION,
            generation,
            files,
        })
    }

    /// Directory holding the previous verified generation.
    pub fn previous_dir(&self) -> PathBuf {
        self.dir.join(PREVIOUS_DIR)
    }

    /// A handle onto the retained previous generation.
    pub fn previous_generation(&self) -> PersistentRuntime {
        PersistentRuntime::new(self.previous_dir())
    }

    /// Copy the current verified set into [`PREVIOUS_DIR`].
    ///
    /// Staged through a temporary directory and swapped in by rename, so an
    /// interrupted snapshot cannot leave a half-copied fallback. A fallback
    /// that is itself torn is worse than no fallback: it looks available and
    /// fails at the moment it is needed.
    fn snapshot_previous_generation(&self) -> Result<(), PersistenceError> {
        let staging = self.dir.join("state.prev.tmp");
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        let staged = PersistentRuntime::new(staging.clone());
        // `backup_to` verifies the source first and copies the manifest, which
        // is exactly the semantics wanted here — reusing it keeps one copy
        // routine rather than two that can drift.
        self.backup_to(&staging)?;

        // Re-verify the copy before it becomes the fallback. Reusing
        // `backup_to`'s verdict would only tell us the *source* was good.
        staged.verify_set_integrity()?;

        let target = self.previous_dir();
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&staging, &target)?;
        Ok(())
    }

    /// Restore the state set from the retained previous generation
    /// (INV-PERSIST-1).
    ///
    /// Returns the generation that was restored. The snapshot is verified
    /// before anything live is touched — repairing from a corrupt fallback
    /// would turn one bad generation into two.
    ///
    /// **This loses the most recent save.** It is recovery, not repair in the
    /// sense of reconstructing the torn generation: the interrupted write is
    /// gone and generation *n-1* takes its place. That is the honest trade, and
    /// it is why this is a separate call rather than something `restore_runtime`
    /// does silently — a caller that would rather investigate a tear than lose
    /// a generation must be able to choose that.
    pub fn repair_from_previous_generation(&self) -> Result<u64, PersistenceError> {
        let previous = self.previous_generation();
        if !previous.exists() {
            return Err(PersistenceError::Integrity(format!(
                "no retained previous generation at {}; nothing to repair from",
                previous.dir.display()
            )));
        }
        match previous.verify_set_integrity()? {
            SetIntegrity::Verified { generation } => {
                self.restore_from(previous.dir.clone())?;
                Ok(generation)
            }
            SetIntegrity::Legacy => Err(PersistenceError::Integrity(format!(
                "retained previous generation at {} has no manifest, so it cannot be \
                 verified before use; refusing to repair from an unverifiable set",
                previous.dir.display()
            ))),
        }
    }

    /// [`PersistentRuntime::restore_runtime`], falling back to the retained
    /// previous generation if the live set is torn (INV-PERSIST-1).
    ///
    /// This is the call that makes a tear survivable rather than fatal. It is
    /// opt-in for the reason above: it silently discards the newest generation,
    /// and a caller that wants to know a tear happened before losing anything
    /// should use `restore_runtime` and decide.
    ///
    /// Returns the state and, when a repair happened, the generation restored.
    pub fn restore_runtime_repairing(
        &self,
    ) -> Result<(RuntimeState, Option<u64>), PersistenceError> {
        match self.restore_runtime() {
            Ok(state) => Ok((state, None)),
            Err(original) => {
                let generation = self.repair_from_previous_generation().map_err(|repair| {
                    // Report the *original* tear as the headline. The repair
                    // failure is context: someone reading this needs to know
                    // what broke first, not just that recovery also failed.
                    PersistenceError::Integrity(format!(
                        "state set is unusable ({original}), and repair from the retained \
                         previous generation also failed ({repair})"
                    ))
                })?;
                let state = self.restore_runtime()?;
                Ok((state, Some(generation)))
            }
        }
    }

    /// Path of the manifest describing the saved set.
    pub fn manifest_path(&self) -> PathBuf {
        self.dir.join(MANIFEST_FILE)
    }

    fn write_manifest(&self, manifest: &StateManifest) -> Result<(), PersistenceError> {
        let bytes = serde_json::to_vec(manifest)?;
        crate::util::write_durable(&self.manifest_path(), &bytes)?;
        Ok(())
    }

    /// Read the manifest, if the directory has one.
    ///
    /// `Ok(None)` means there is no manifest (a legacy directory).
    /// `Err(..)` means there is one and it could not be read or parsed — a
    /// corrupt manifest is a failure, not an invitation to proceed unverified.
    pub fn read_manifest(&self) -> Result<Option<StateManifest>, PersistenceError> {
        let path = self.manifest_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        let manifest: StateManifest = serde_json::from_slice(&bytes)?;
        Ok(Some(manifest))
    }

    /// Check the state directory against its manifest without loading or
    /// deserializing the state itself (INV-PERSIST-1).
    ///
    /// Returns [`SetIntegrity::Verified`] when every recorded digest matches,
    /// [`SetIntegrity::Legacy`] when there is no manifest to check against, and
    /// an [`PersistenceError::Integrity`] when the set is torn: a file whose
    /// contents do not match what the manifest recorded, or a member file that
    /// has gone missing.
    pub fn verify_set_integrity(&self) -> Result<SetIntegrity, PersistenceError> {
        let Some(manifest) = self.read_manifest()? else {
            return Ok(SetIntegrity::Legacy);
        };

        if manifest.format_version > STATE_FORMAT_VERSION {
            return Err(PersistenceError::Integrity(format!(
                "state directory declares format version {} but this build understands at most {}; \
                 refusing to read it rather than guess at the layout",
                manifest.format_version, STATE_FORMAT_VERSION
            )));
        }

        for (name, expected) in &manifest.files {
            let path = self.dir.join(name);
            let bytes = std::fs::read(&path).map_err(|e| {
                PersistenceError::Integrity(format!(
                    "state file '{name}' is named in the manifest (generation {}) but could not \
                     be read: {e}",
                    manifest.generation
                ))
            })?;
            let actual = digest_hex(&bytes);
            if &actual != expected {
                return Err(PersistenceError::Integrity(format!(
                    "state file '{name}' does not match the manifest (generation {}): expected \
                     sha256 {expected}, found {actual}. The set is torn — a save was interrupted \
                     between files, or a file was modified out of band.",
                    manifest.generation
                )));
            }
        }

        Ok(SetIntegrity::Verified {
            generation: manifest.generation,
        })
    }

    /// Load state from the directory (no validation).
    pub fn load_state(&self) -> Result<RuntimeState, PersistenceError> {
        let graph: MemoryGraph = read_json(&self.graph_path())?;
        let event_log: EventLog = read_json(&self.events_path())?;
        let dist_log: DistributedEventLog = read_json(&self.memory_path())?;
        Ok(KernelSnapshot::new(graph, event_log, dist_log))
    }

    /// Load **and verify** the runtime: the file set must match its manifest,
    /// the hash chain must validate, and the graph must hold no dangling edges.
    /// Returns the ready-to-run state.
    ///
    /// The set check runs *first* and reads only digests. A torn set is
    /// reported as a torn set, rather than surfacing later as a confusing
    /// chain-verification failure in whichever file happened to be stale.
    ///
    /// A directory with no manifest is accepted as legacy — see
    /// [`SetIntegrity::Legacy`] for why, and use
    /// [`PersistentRuntime::restore_runtime_strict`] when that is not
    /// acceptable.
    pub fn restore_runtime(&self) -> Result<RuntimeState, PersistenceError> {
        self.verify_set_integrity()?;
        let state = self.load_state()?;
        state
            .verify_integrity()
            .map_err(PersistenceError::Integrity)?;
        Ok(state)
    }

    /// [`PersistentRuntime::restore_runtime`], but a directory without a
    /// manifest is refused instead of accepted as legacy.
    ///
    /// Use this where the state directory is not fully trusted: a missing
    /// manifest and a deleted one look identical from here, so accepting
    /// "legacy" lets anyone who can unlink a file opt the directory out of
    /// verification.
    pub fn restore_runtime_strict(&self) -> Result<RuntimeState, PersistenceError> {
        if self.verify_set_integrity()? == SetIntegrity::Legacy {
            return Err(PersistenceError::Integrity(format!(
                "state directory {} has no {MANIFEST_FILE}; strict restore will not accept an \
                 unverifiable set. Re-save with this build to produce one.",
                self.dir.display()
            )));
        }
        self.restore_runtime()
    }

    /// Copy a verified snapshot of this state directory into `dest`
    /// (INV-PERSIST-2).
    ///
    /// The source is verified **before** anything is copied: a backup of a
    /// known-torn set is worse than no backup, because it looks like a
    /// recovery option and is not. Files are written durably, and the manifest
    /// is copied last for the same reason `save_state` writes it last — an
    /// interrupted backup is then detectable in the backup directory too.
    pub fn backup_to(&self, dest: impl AsRef<Path>) -> Result<SetIntegrity, PersistenceError> {
        let dest = dest.as_ref();
        let integrity = self.verify_set_integrity()?;
        std::fs::create_dir_all(dest)?;

        for path in [self.graph_path(), self.events_path(), self.memory_path()] {
            let bytes = std::fs::read(&path)?;
            crate::util::write_durable(&dest.join(file_name_of(&path)?), &bytes)?;
        }
        if let Some(manifest) = self.read_manifest()? {
            let bytes = serde_json::to_vec(&manifest)?;
            crate::util::write_durable(&dest.join(MANIFEST_FILE), &bytes)?;
        }
        Ok(integrity)
    }

    /// Restore this state directory from a backup previously produced by
    /// [`PersistentRuntime::backup_to`] (INV-PERSIST-2).
    ///
    /// The *source* is verified before any file in `self.dir` is touched. A
    /// restore that overwrites live state with a corrupt backup turns a
    /// recoverable incident into an unrecoverable one, so the check cannot come
    /// afterwards.
    pub fn restore_from(&self, src: impl AsRef<Path>) -> Result<SetIntegrity, PersistenceError> {
        let source = PersistentRuntime::new(src.as_ref().to_path_buf());
        let integrity = source.verify_set_integrity()?;
        if !source.exists() {
            return Err(PersistenceError::Integrity(format!(
                "backup directory {} does not contain a complete state set",
                src.as_ref().display()
            )));
        }

        std::fs::create_dir_all(&self.dir)?;
        for path in [
            source.graph_path(),
            source.events_path(),
            source.memory_path(),
        ] {
            let bytes = std::fs::read(&path)?;
            crate::util::write_durable(&self.dir.join(file_name_of(&path)?), &bytes)?;
        }
        match source.read_manifest()? {
            Some(manifest) => self.write_manifest(&manifest)?,
            // The source is legacy, so the destination must not keep a stale
            // manifest from its own previous life: it would describe digests
            // that the just-restored files do not have, and every later restore
            // would report a tear that is not there.
            None => {
                let path = self.manifest_path();
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            }
        }
        Ok(integrity)
    }
}

/// The file name component of `path`, as a `String`.
fn file_name_of(path: &Path) -> Result<String, PersistenceError> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| {
            PersistenceError::Io(format!(
                "state path has no usable file name: {}",
                path.display()
            ))
        })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), PersistenceError> {
    let json = serde_json::to_string(value)?;
    crate::util::write_durable(path, json.as_bytes())?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, PersistenceError> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::{EventPayload, EventType};
    use crate::memory::{EdgeType, NodeType};

    fn temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "ccos_persist_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    fn sample_state() -> RuntimeState {
        let mut graph = MemoryGraph::default();
        for i in 0..8 {
            graph.upsert_node(
                format!("n{i}").into(),
                format!("L{i}"),
                "content".into(),
                NodeType::Module,
            );
        }
        graph.add_edge("n0".into(), "n1".into(), 0.9, EdgeType::DependsOn);
        graph.add_edge("n1".into(), "n2".into(), 0.8, EdgeType::Contains);

        let mut event_log = EventLog::new("persist-runtime".into());
        for c in 0..5 {
            event_log.append(
                EventType::CycleEnd,
                EventPayload::CycleEvent {
                    cycle_number: c,
                    action: format!("cycle_{c}"),
                },
            );
        }

        let mut dist_log = DistributedEventLog::new();
        for c in 0..5 {
            dist_log.append(format!("event_{c}"), "kernel".into());
        }

        RuntimeState::new(graph, event_log, dist_log)
    }

    #[test]
    fn save_then_restore_reproduces_state() {
        let dir = temp_dir("roundtrip");
        let runtime = PersistentRuntime::new(&dir);

        // 1. Create state.
        let before = sample_state();
        let (n0, e0, ev0, chain0) = (
            before.graph.node_count(),
            before.graph.edge_count(),
            before.event_log.event_count(),
            before.dist_log.event_count(),
        );

        // 2. Persist and simulate shutdown (drop the in-memory state).
        runtime.save_state(&before).unwrap();
        drop(before);
        assert!(runtime.exists());

        // 3. Reload on a fresh runtime handle.
        let after = PersistentRuntime::new(&dir).restore_runtime().unwrap();

        // 4. Compare before/after.
        assert_eq!(after.graph.node_count(), n0);
        assert_eq!(after.graph.edge_count(), e0);
        assert_eq!(after.event_log.event_count(), ev0);
        assert_eq!(after.dist_log.event_count(), chain0);
        assert!(after.dist_log.verify_integrity().valid);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restore_detects_tampered_chain() {
        let dir = temp_dir("tamper");
        let runtime = PersistentRuntime::new(&dir);
        let mut state = sample_state();
        // Corrupt a hash-chain link before saving.
        state.dist_log.hash_chain[2].hash = "deadbeef".repeat(8);
        runtime.save_state(&state).unwrap();

        let restored = runtime.restore_runtime();
        assert!(
            matches!(restored, Err(PersistenceError::Integrity(_))),
            "tampered chain must be rejected on restore"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_missing_state_errors_cleanly() {
        let dir = temp_dir("missing");
        let runtime = PersistentRuntime::new(&dir);
        assert!(!runtime.exists());
        assert!(matches!(runtime.load_state(), Err(PersistenceError::Io(_))));
    }

    // ── Set-level integrity, backup and restore (P1-7 / INV-PERSIST-1/2) ──

    fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_saved_set_verifies_against_its_manifest() {
        let dir = temp_dir("verify_ok");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();

        assert_eq!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 0 },
        );
        assert!(runtime.manifest_path().exists());
        cleanup(&dir);
    }

    #[test]
    fn the_generation_counter_advances_on_each_save() {
        let dir = temp_dir("generation");
        let runtime = PersistentRuntime::new(&dir);
        let state = sample_state();

        runtime.save_state(&state).unwrap();
        runtime.save_state(&state).unwrap();
        runtime.save_state(&state).unwrap();

        assert_eq!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 2 },
            "a restore of an older backup should be visible as a lower generation"
        );
        cleanup(&dir);
    }

    /// The acceptance test for INV-PERSIST-1: a crash between two of the three
    /// renames leaves every file individually intact but the set describing a
    /// moment that never existed. Simulated by replacing one member file after
    /// the save, which is byte-for-byte the state such a crash leaves behind.
    #[test]
    fn a_torn_multi_file_write_is_detected() {
        let dir = temp_dir("torn");
        let runtime = PersistentRuntime::new(&dir);

        // Generation 0: the set the manifest will describe.
        runtime.save_state(&sample_state()).unwrap();
        assert!(runtime.verify_set_integrity().is_ok());

        // Now write a *newer, individually valid* graph over the old one,
        // without updating the manifest — exactly what a crash after the first
        // rename leaves on disk.
        let mut newer = sample_state();
        newer.graph.upsert_node(
            "n99".into(),
            "L99".into(),
            "added after the manifest was written".into(),
            NodeType::Module,
        );
        crate::util::write_durable(
            &runtime.graph_path(),
            &serde_json::to_vec(&newer.graph).unwrap(),
        )
        .unwrap();

        let err = runtime
            .verify_set_integrity()
            .expect_err("a torn set must be detected, not loaded");
        let msg = err.to_string();
        assert!(msg.contains("graph.snapshot"), "names the file: {msg}");
        assert!(msg.contains("torn"), "explains what happened: {msg}");

        // And the tear must stop `restore_runtime`, not merely be reportable.
        assert!(
            runtime.restore_runtime().is_err(),
            "restore must refuse a torn set"
        );
        cleanup(&dir);
    }

    #[test]
    fn a_missing_member_file_is_detected_rather_than_read_as_absent() {
        let dir = temp_dir("missing");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        std::fs::remove_file(runtime.events_path()).unwrap();

        let err = runtime
            .verify_set_integrity()
            .expect_err("must be detected");
        assert!(err.to_string().contains("events.log"), "{err}");
        cleanup(&dir);
    }

    /// The acceptance test for INV-PERSIST-2: take a backup, destroy the live
    /// state, restore, and prove the restored runtime both verifies and holds
    /// the original data.
    #[test]
    fn backup_destroy_restore_recovers_verified_state() {
        let live_dir = temp_dir("bdr_live");
        let backup_dir = temp_dir("bdr_backup");
        let runtime = PersistentRuntime::new(&live_dir);

        let before = sample_state();
        let (nodes, edges, events, chain) = (
            before.graph.node_count(),
            before.graph.edge_count(),
            before.event_log.event_count(),
            before.dist_log.event_count(),
        );
        runtime.save_state(&before).unwrap();

        // 1. Back up, and confirm the backup itself is a verified set rather
        //    than a directory that merely has files in it.
        runtime.backup_to(&backup_dir).unwrap();
        let backup = PersistentRuntime::new(&backup_dir);
        assert_eq!(
            backup.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 0 },
        );

        // 2. Destroy the live state completely.
        std::fs::remove_dir_all(&live_dir).unwrap();
        assert!(!runtime.exists(), "live state is gone");

        // 3. Restore from the backup.
        runtime.restore_from(&backup_dir).unwrap();

        // 4. Integrity check: the set verifies AND the state loads and passes
        //    its own hash-chain/dangling-edge verification.
        assert_eq!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 0 },
        );
        let after = runtime.restore_runtime().unwrap();
        assert_eq!(after.graph.node_count(), nodes);
        assert_eq!(after.graph.edge_count(), edges);
        assert_eq!(after.event_log.event_count(), events);
        assert_eq!(after.dist_log.event_count(), chain);

        cleanup(&live_dir);
        cleanup(&backup_dir);
    }

    #[test]
    fn a_backup_of_a_torn_set_is_refused_rather_than_taken() {
        // A backup of a known-torn set is worse than no backup: it looks like
        // a recovery option and is not.
        let live_dir = temp_dir("badbackup_live");
        let backup_dir = temp_dir("badbackup_dest");
        let runtime = PersistentRuntime::new(&live_dir);
        runtime.save_state(&sample_state()).unwrap();
        crate::util::write_durable(&runtime.events_path(), b"{}").unwrap();

        assert!(runtime.backup_to(&backup_dir).is_err());
        assert!(
            !backup_dir.join(MANIFEST_FILE).exists(),
            "nothing should have been written to the backup directory"
        );
        cleanup(&live_dir);
        cleanup(&backup_dir);
    }

    #[test]
    fn a_corrupt_backup_does_not_overwrite_live_state() {
        let live_dir = temp_dir("restore_live");
        let backup_dir = temp_dir("restore_backup");
        let runtime = PersistentRuntime::new(&live_dir);
        let backup = PersistentRuntime::new(&backup_dir);

        runtime.save_state(&sample_state()).unwrap();
        runtime.backup_to(&backup_dir).unwrap();

        // Corrupt the backup after it was taken.
        crate::util::write_durable(&backup.graph_path(), b"{\"nodes\":{}}").unwrap();

        assert!(
            runtime.restore_from(&backup_dir).is_err(),
            "restoring from a corrupt backup must fail"
        );
        // The live state must be exactly as it was — a failed restore that has
        // already half-overwritten the live directory turns a recoverable
        // incident into an unrecoverable one.
        assert_eq!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 0 },
        );
        assert!(runtime.restore_runtime().is_ok());
        cleanup(&live_dir);
        cleanup(&backup_dir);
    }

    #[test]
    fn a_legacy_directory_without_a_manifest_still_loads() {
        // Upgrading must not strand an existing state directory.
        let dir = temp_dir("legacy");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        std::fs::remove_file(runtime.manifest_path()).unwrap();

        assert_eq!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Legacy
        );
        assert!(
            runtime.restore_runtime().is_ok(),
            "a legacy set must remain loadable"
        );
        cleanup(&dir);
    }

    #[test]
    fn strict_restore_refuses_a_directory_with_no_manifest() {
        // Because a missing manifest and a deleted one are indistinguishable,
        // accepting "legacy" lets anyone who can unlink a file opt out of
        // verification. Strict mode is the answer to that, and it must
        // actually refuse.
        let dir = temp_dir("strict");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        std::fs::remove_file(runtime.manifest_path()).unwrap();

        let err = runtime
            .restore_runtime_strict()
            .expect_err("strict restore must refuse an unverifiable set");
        assert!(err.to_string().contains(MANIFEST_FILE), "{err}");

        // But it accepts a properly saved one.
        runtime.save_state(&sample_state()).unwrap();
        assert!(runtime.restore_runtime_strict().is_ok());
        cleanup(&dir);
    }

    #[test]
    fn restoring_a_legacy_backup_clears_a_stale_destination_manifest() {
        // Otherwise the destination keeps a manifest describing digests the
        // just-restored files do not have, and every later verification
        // reports a tear that is not there.
        let live_dir = temp_dir("stale_live");
        let backup_dir = temp_dir("stale_backup");
        let runtime = PersistentRuntime::new(&live_dir);
        let backup = PersistentRuntime::new(&backup_dir);

        // A legacy backup: files, no manifest.
        backup.save_state(&sample_state()).unwrap();
        std::fs::remove_file(backup.manifest_path()).unwrap();

        // A live directory that DOES have a manifest, from its own history.
        let mut other = sample_state();
        other
            .graph
            .upsert_node("zz".into(), "ZZ".into(), "x".into(), NodeType::Module);
        runtime.save_state(&other).unwrap();
        assert!(runtime.manifest_path().exists());

        runtime.restore_from(&backup_dir).unwrap();

        assert_eq!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Legacy,
            "the stale manifest must be gone, not left to contradict the files"
        );
        assert!(runtime.restore_runtime().is_ok());
        cleanup(&live_dir);
        cleanup(&backup_dir);
    }

    #[test]
    fn a_future_format_version_is_refused_rather_than_guessed_at() {
        let dir = temp_dir("future");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();

        let mut manifest = runtime.read_manifest().unwrap().unwrap();
        manifest.format_version = STATE_FORMAT_VERSION + 1;
        crate::util::write_durable(
            &runtime.manifest_path(),
            &serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();

        let err = runtime.verify_set_integrity().expect_err("must refuse");
        assert!(err.to_string().contains("format version"), "{err}");
        cleanup(&dir);
    }

    #[test]
    fn a_corrupt_manifest_is_an_error_not_a_downgrade_to_legacy() {
        // If an unparseable manifest fell back to "legacy", corrupting it
        // would be a way to disable verification entirely.
        let dir = temp_dir("badmanifest");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        crate::util::write_durable(&runtime.manifest_path(), b"not json at all").unwrap();

        assert!(runtime.read_manifest().is_err());
        assert!(runtime.verify_set_integrity().is_err());
        cleanup(&dir);
    }

    // ── P1-7-B: repair from the retained previous generation ────────────────

    /// Tear the live graph file without updating the manifest — what a crash
    /// after the first rename leaves on disk.
    fn tear_the_live_set(runtime: &PersistentRuntime) {
        let mut newer = sample_state();
        newer.graph.upsert_node(
            "torn".into(),
            "Ltorn".into(),
            "written after the manifest".into(),
            NodeType::Module,
        );
        crate::util::write_durable(
            &runtime.graph_path(),
            &serde_json::to_vec(&newer.graph).unwrap(),
        )
        .unwrap();
    }

    /// P1-7-B acceptance test: a torn set is **repaired** from the previous
    /// generation rather than only reported.
    #[test]
    fn a_torn_set_is_repaired_from_the_previous_generation() {
        let dir = temp_dir("repair");
        let runtime = PersistentRuntime::new(&dir);

        runtime.save_state(&sample_state()).unwrap(); // generation 0
        runtime.save_state(&sample_state()).unwrap(); // generation 1, snapshots 0

        assert!(
            runtime.previous_generation().exists(),
            "the second save must retain the first generation"
        );

        tear_the_live_set(&runtime);
        assert!(
            runtime.restore_runtime().is_err(),
            "the tear must still be detected — repair is a separate decision"
        );

        let (state, repaired) = runtime
            .restore_runtime_repairing()
            .expect("a torn set with a good fallback must be recoverable");
        assert_eq!(
            repaired,
            Some(0),
            "it must report which generation it fell back to, not repair silently"
        );
        state
            .verify_integrity()
            .expect("repaired state must verify");
        assert!(matches!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 0 }
        ));
    }

    /// A clean set must not be touched, and must not claim a repair happened.
    #[test]
    fn an_intact_set_is_not_repaired() {
        let dir = temp_dir("repair_noop");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        runtime.save_state(&sample_state()).unwrap();

        let (_state, repaired) = runtime.restore_runtime_repairing().unwrap();
        assert_eq!(repaired, None, "no tear means no repair");
        assert!(matches!(
            runtime.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 1 }
        ));
    }

    /// The first save has no previous generation, so a tear then is genuinely
    /// unrecoverable — and must say so rather than appear to succeed.
    #[test]
    fn a_tear_before_any_second_save_is_reported_as_unrecoverable() {
        let dir = temp_dir("repair_none");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        tear_the_live_set(&runtime);

        let err = runtime.restore_runtime_repairing().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("torn"), "leads with the original tear: {msg}");
        assert!(
            msg.contains("repair") || msg.contains("nothing to repair from"),
            "and says recovery was attempted: {msg}"
        );
    }

    /// Repairing from a fallback that is itself corrupt would turn one bad
    /// generation into two. The snapshot is verified before anything live is
    /// touched.
    #[test]
    fn a_corrupt_fallback_is_refused_and_the_live_set_is_left_alone() {
        let dir = temp_dir("repair_badprev");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        runtime.save_state(&sample_state()).unwrap();

        // Corrupt the retained generation, then tear the live one.
        let previous = runtime.previous_generation();
        crate::util::write_durable(&previous.graph_path(), b"{}").unwrap();
        tear_the_live_set(&runtime);

        let err = runtime.restore_runtime_repairing().unwrap_err();
        assert!(
            err.to_string().contains("repair"),
            "must report that recovery was tried and failed: {err}"
        );
        // The live set is still torn, not overwritten with the bad fallback.
        assert!(runtime.verify_set_integrity().is_err());
    }

    /// A save that finds the live set already torn must not overwrite a good
    /// fallback with a bad one: the older good generation is worth more than
    /// the newer broken one.
    #[test]
    fn saving_over_a_torn_set_does_not_destroy_the_good_fallback() {
        let dir = temp_dir("repair_keepgood");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap(); // gen 0
        runtime.save_state(&sample_state()).unwrap(); // gen 1, .prev = gen 0

        tear_the_live_set(&runtime);
        runtime.save_state(&sample_state()).unwrap(); // gen 2, must NOT snapshot the torn gen 1

        let previous = runtime.previous_generation();
        assert!(
            matches!(
                previous.verify_set_integrity().unwrap(),
                SetIntegrity::Verified { generation: 0 }
            ),
            "the retained generation must still be the last verified one"
        );
    }

    /// The snapshot must be verifiable in its own right, not merely a copy of
    /// something that was verified at the time.
    #[test]
    fn the_retained_generation_verifies_independently() {
        let dir = temp_dir("repair_verifies");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();
        runtime.save_state(&sample_state()).unwrap();

        let previous = runtime.previous_generation();
        assert!(matches!(
            previous.verify_set_integrity().unwrap(),
            SetIntegrity::Verified { generation: 0 }
        ));
        previous
            .restore_runtime()
            .expect("the fallback must be loadable on its own");
    }

    /// Staging must not leave debris that a later snapshot trips over.
    #[test]
    fn a_leftover_staging_directory_does_not_block_the_next_snapshot() {
        let dir = temp_dir("repair_staging");
        let runtime = PersistentRuntime::new(&dir);
        runtime.save_state(&sample_state()).unwrap();

        // Simulate an interrupted snapshot.
        let staging = dir.join("state.prev.tmp");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("junk"), b"debris").unwrap();

        runtime.save_state(&sample_state()).unwrap();
        assert!(
            !staging.exists(),
            "staging must be cleared, not accumulated"
        );
        assert!(runtime.previous_generation().exists());
    }
}
