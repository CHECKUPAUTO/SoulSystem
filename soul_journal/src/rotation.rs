//! Journal rotatif : enchaine des segments `MmapJournal` de taille fixe.
//! Fast-path concurrent (read-lock bref + append lock-free dans le segment) ;
//! la rotation se fait sous write-lock (rare, exclusive). Les segments scelles
//! restent mappes (Arc vivant) tant que le `RotatingJournal` vit -> lecture
//! multi-segments sans use-after-free.

use std::sync::{Arc, RwLock};
use crate::MmapJournal;

const DEFAULT_SEGMENT_SIZE: usize = 1024 * 1024 * 64;

struct RotState {
    current: Arc<MmapJournal>,
    sealed: Vec<Arc<MmapJournal>>,
    seg_index: u32,
}

pub struct RotatingJournal {
    base_path: String,
    segment_size: usize,
    state: RwLock<RotState>,
}

impl RotatingJournal {
    pub fn new(base_path: &str) -> std::io::Result<Self> {
        Self::new_with_size(base_path, DEFAULT_SEGMENT_SIZE)
    }

    pub fn new_with_size(base_path: &str, segment_size: usize) -> std::io::Result<Self> {
        let seg0 = Arc::new(MmapJournal::new_with_size(&seg_path(base_path, 0), segment_size)?);
        Ok(Self {
            base_path: base_path.to_string(),
            segment_size,
            state: RwLock::new(RotState { current: seg0, sealed: Vec::new(), seg_index: 0 }),
        })
    }

    /// Ajoute un record ; bascule sur un nouveau segment si le courant est plein.
    /// `false` uniquement si la donnee est invalide (vide ou >= taille de segment)
    /// ou si la creation du nouveau segment echoue.
    pub fn append_log(&self, tag: u32, data: &[u8]) -> bool {
        if data.is_empty() || data.len() + 8 >= self.segment_size {
            return false;
        }
        let cur = self.state.read().unwrap().current.clone();
        if cur.append_log(tag, data) {
            return true;
        }
        let mut st = self.state.write().unwrap();
        if st.current.append_log(tag, data) {
            return true; // un autre thread avait deja tourne
        }
        let next_idx = st.seg_index + 1;
        let seg = match MmapJournal::new_with_size(&seg_path(&self.base_path, next_idx), self.segment_size) {
            Ok(s) => Arc::new(s),
            Err(_) => return false,
        };
        let old = std::mem::replace(&mut st.current, seg);
        old.sync();
        st.sealed.push(old);
        st.seg_index = next_idx;
        st.current.append_log(tag, data)
    }

    /// Relit tous les records commites (segments scelles puis courant, dans l'ordre).
    pub fn read_all_committed(&self) -> Vec<(u32, Vec<u8>)> {
        let st = self.state.read().unwrap();
        let mut out = Vec::new();
        for seg in &st.sealed {
            out.extend(seg.read_committed());
        }
        out.extend(st.current.read_committed());
        out
    }

    /// msync de tous les segments.
    pub fn sync_all(&self) -> bool {
        let st = self.state.read().unwrap();
        let mut ok = true;
        for seg in &st.sealed {
            ok &= seg.sync();
        }
        ok & st.current.sync()
    }

    /// Nombre de segments (scelles + courant).
    pub fn segment_count(&self) -> usize {
        self.state.read().unwrap().sealed.len() + 1
    }
}

#[inline]
fn seg_path(base: &str, idx: u32) -> String {
    format!("{}.{:04}", base, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_sur_segment_plein() {
        let base = format!("/tmp/soul_rotjournal_{}", std::process::id());
        for i in 0u32..64 { let _ = std::fs::remove_file(seg_path(&base, i)); }

        let j = RotatingJournal::new_with_size(&base, 8 * 1024).expect("create rotating");
        let payload = [0xABu8; 256];
        let n = 500u32;
        for tag in 1..=n {
            assert!(j.append_log(tag, &payload), "append {} a echoue", tag);
        }
        assert!(j.sync_all());
        assert!(j.segment_count() >= 2, "rotation attendue (count={})", j.segment_count());

        let recs = j.read_all_committed();
        assert_eq!(recs.len(), n as usize, "tous les records relus a travers les segments");
        for (i, (tag, p)) in recs.iter().enumerate() {
            assert_eq!(*tag, i as u32 + 1, "ordre des records");
            assert_eq!(p.len(), 256);
            assert!(p.iter().all(|&b| b == 0xAB));
        }
        println!("PREUVE rotation : {} records / {} segments, relecture intacte", n, j.segment_count());
        for i in 0u32..64 { let _ = std::fs::remove_file(seg_path(&base, i)); }
    }

    #[test]
    fn record_trop_grand_refuse_sans_boucle() {
        let base = format!("/tmp/soul_rotjournal_big_{}", std::process::id());
        for i in 0u32..4 { let _ = std::fs::remove_file(seg_path(&base, i)); }
        let j = RotatingJournal::new_with_size(&base, 4 * 1024).expect("create");
        let huge = vec![1u8; 8 * 1024];
        assert!(!j.append_log(1, &huge), "record plus grand qu'un segment -> refuse");
        for i in 0u32..4 { let _ = std::fs::remove_file(seg_path(&base, i)); }
    }
}
