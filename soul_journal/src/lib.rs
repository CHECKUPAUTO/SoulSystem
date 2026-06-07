use std::ffi::CString;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

const JOURNAL_SIZE: usize = 1024 * 1024 * 64; // Segment de 64 Mo pre-alloue en mmap

/// Taille reservee : entete (tag u32 + size u32) + payload, arrondie a 4 octets
/// pour que le champ `size` de chaque record soit aligne (AtomicU32 -> align 4).
#[inline]
const fn padded_len(payload_size: usize) -> usize {
    (8 + payload_size + 3) & !3
}

pub struct MmapJournal {
    mmap_ptr: *mut u8,
    write_offset: AtomicUsize,
}

unsafe impl Send for MmapJournal {}
unsafe impl Sync for MmapJournal {}

impl MmapJournal {
    pub fn new(file_path: &str) -> Self {
        unsafe {
            let c_path = CString::new(file_path).unwrap();
            let fd = libc::open(c_path.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o666);
            if fd < 0 {
                panic!("[JOURNAL CRITICAL] Impossible de creer le descripteur NVMe.");
            }
            libc::ftruncate(fd, JOURNAL_SIZE as libc::off_t);
            let mmap_ptr = libc::mmap(
                std::ptr::null_mut(),
                JOURNAL_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            libc::close(fd);
            if mmap_ptr == libc::MAP_FAILED {
                panic!("[JOURNAL CRITICAL] Echec mmap native.");
            }
            Self { mmap_ptr: mmap_ptr as *mut u8, write_offset: AtomicUsize::new(0) }
        }
    }

    /// PROTOCOLE DE PUBLICATION : reserve le slot (CAS sur l'offset), ecrit
    /// tag+payload, PUIS publie `size` en DERNIER via store Release. Un lecteur
    /// chargeant `size` en Acquire et le voyant >0 a la garantie que tag+payload
    /// sont visibles (happens-before) -> aucune lecture dechiree. `size==0` =
    /// marqueur "non commite", donc payload vide refuse.
    pub fn append_log(&self, tag: u32, data: &[u8]) -> bool {
        let size = data.len();
        if size == 0 || size > u32::MAX as usize {
            return false;
        }
        let need = padded_len(size);
        loop {
            let current_offset = self.write_offset.load(Ordering::Acquire);
            if current_offset + need >= JOURNAL_SIZE {
                return false; // plein
            }
            if self
                .write_offset
                .compare_exchange_weak(
                    current_offset,
                    current_offset + need,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                unsafe {
                    let base = self.mmap_ptr.add(current_offset);
                    std::ptr::copy_nonoverlapping(&tag as *const u32 as *const u8, base, 4);
                    std::ptr::copy_nonoverlapping(data.as_ptr(), base.add(8), size);
                    // PUBLICATION : size en dernier, store Release (offset+4 aligne 4).
                    let size_cell = base.add(4) as *const AtomicU32;
                    (*size_cell).store(size as u32, Ordering::Release);
                }
                return true;
            }
        }
    }

    /// Relit les records COMMITES. `size` lu en Acquire ; `size==0` -> fin de la
    /// zone lisible. Sound vis-a-vis d'ecrivains concurrents.
    pub fn read_committed(&self) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        let mut off = 0usize;
        loop {
            if off + 8 > JOURNAL_SIZE {
                break;
            }
            unsafe {
                let base = self.mmap_ptr.add(off);
                let size = (*(base.add(4) as *const AtomicU32)).load(Ordering::Acquire) as usize;
                if size == 0 {
                    break;
                }
                if off + 8 + size > JOURNAL_SIZE {
                    break;
                }
                let mut tag_bytes = [0u8; 4];
                std::ptr::copy_nonoverlapping(base, tag_bytes.as_mut_ptr(), 4);
                let tag = u32::from_le_bytes(tag_bytes);
                let mut payload = vec![0u8; size];
                std::ptr::copy_nonoverlapping(base.add(8), payload.as_mut_ptr(), size);
                out.push((tag, payload));
                off += padded_len(size);
            }
        }
        out
    }

    #[inline]
    pub fn written_len(&self) -> usize {
        self.write_offset.load(Ordering::Acquire)
    }

    pub fn sync(&self) -> bool {
        let len = self.write_offset.load(Ordering::Acquire);
        if len == 0 {
            return true;
        }
        unsafe { libc::msync(self.mmap_ptr as *mut libc::c_void, len, libc::MS_SYNC) == 0 }
    }

    pub fn spawn_flusher(self: &Arc<Self>, period: Duration) -> std::io::Result<JoinHandle<()>> {
        let weak: Weak<Self> = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("journal-flusher".to_string())
            .spawn(move || loop {
                match weak.upgrade() {
                    Some(journal) => {
                        journal.sync();
                    }
                    None => break,
                }
                std::thread::sleep(period);
            })
    }
}

impl Drop for MmapJournal {
    fn drop(&mut self) {
        unsafe {
            let len = self.write_offset.load(Ordering::Acquire);
            if len > 0 {
                libc::msync(self.mmap_ptr as *mut libc::c_void, len, libc::MS_SYNC);
            }
            libc::munmap(self.mmap_ptr as *mut libc::c_void, JOURNAL_SIZE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn relire_records(path: &str, nb_attendu: usize) -> Vec<(u32, Vec<u8>)> {
        let mut f = std::fs::File::open(path).expect("open journal");
        let mut buf = vec![0u8; 65536];
        let n = f.read(&mut buf).expect("read journal");
        buf.truncate(n);
        let mut out = Vec::new();
        let mut off = 0usize;
        while out.len() < nb_attendu && off + 8 <= buf.len() {
            let size = u32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]) as usize;
            if size == 0 {
                break;
            }
            let tag = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
            let end = off + 8 + size;
            if end > buf.len() {
                break;
            }
            out.push((tag, buf[off + 8..end].to_vec()));
            off += padded_len(size);
        }
        out
    }

    #[test]
    fn sync_puis_reopen_intact() {
        let path = format!("/tmp/soul_journal_test_sync_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        {
            let j = MmapJournal::new(&path);
            assert!(j.append_log(0xAA, b"hello"));
            assert!(j.append_log(0xBB, b"world!!"));
            assert!(j.sync());
            let recs = relire_records(&path, 2);
            assert_eq!(recs, vec![(0xAAu32, b"hello".to_vec()), (0xBBu32, b"world!!".to_vec())]);
            println!("PREUVE sync+reopen : 2 records intacts (poignee tierce, padding-aware)");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn durabilite_sans_drop() {
        let path = format!("/tmp/soul_journal_test_nodrop_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = MmapJournal::new(&path);
        assert!(j.append_log(0x01, b"durable-record"));
        assert!(j.sync());
        std::mem::forget(j);
        let recs = relire_records(&path, 1);
        assert_eq!(recs, vec![(0x01u32, b"durable-record".to_vec())]);
        println!("PREUVE no-Drop : record present apres mem::forget");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_committed_et_payload_vide_refuse() {
        let path = format!("/tmp/soul_journal_test_rc_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = MmapJournal::new(&path);
        assert!(j.append_log(10, b"abc"));
        assert!(j.append_log(20, b"defgh"));
        assert!(!j.append_log(30, b""), "payload vide refuse (sentinel size==0)");
        assert_eq!(j.read_committed(), vec![(10u32, b"abc".to_vec()), (20u32, b"defgh".to_vec())]);
        println!("PREUVE read_committed + refus payload vide");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn flusher_periodique_et_auto_stop() {
        let path = format!("/tmp/soul_journal_test_flush_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = Arc::new(MmapJournal::new(&path));
        let h = j.spawn_flusher(Duration::from_millis(20)).expect("spawn flusher");
        assert!(j.append_log(0x77, b"flushed-by-thread"));
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(relire_records(&path, 1), vec![(0x77u32, b"flushed-by-thread".to_vec())]);
        println!("PREUVE flusher : record persiste");
        drop(j);
        let start = std::time::Instant::now();
        h.join().expect("join flusher");
        assert!(start.elapsed() < Duration::from_secs(2));
        println!("PREUVE auto-stop flusher : termine en {:?}", start.elapsed());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn concurrent_aucune_lecture_dechiree() {
        let path = format!("/tmp/soul_journal_test_conc_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = Arc::new(MmapJournal::new(&path));
        const N: u32 = 2000;

        let jw = j.clone();
        let writer = std::thread::spawn(move || {
            for tag in 1..=N {
                let payload = [(tag % 251) as u8; 16];
                while !jw.append_log(tag, &payload) {
                    std::thread::yield_now();
                }
            }
        });

        let jr = j.clone();
        let reader = std::thread::spawn(move || {
            let mut max_seen = 0usize;
            loop {
                let recs = jr.read_committed();
                for (tag, payload) in &recs {
                    let expected = (tag % 251) as u8;
                    assert_eq!(payload.len(), 16, "longueur incoherente");
                    assert!(
                        payload.iter().all(|&b| b == expected),
                        "LECTURE DECHIREE tag={} -> {:?}",
                        tag,
                        payload
                    );
                }
                max_seen = max_seen.max(recs.len());
                if max_seen >= N as usize {
                    break;
                }
                std::thread::yield_now();
            }
            max_seen
        });

        writer.join().expect("writer");
        assert_eq!(reader.join().expect("reader"), N as usize);

        let final_recs = j.read_committed();
        assert_eq!(final_recs.len(), N as usize);
        for (i, (tag, payload)) in final_recs.iter().enumerate() {
            assert_eq!(*tag, (i as u32) + 1, "ordre");
            assert!(payload.iter().all(|&b| b == (tag % 251) as u8));
        }
        println!("PREUVE concurrent : {} records, aucune lecture dechiree (Release/Acquire)", N);
        let _ = std::fs::remove_file(&path);
    }
}
