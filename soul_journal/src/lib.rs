use std::ffi::CString;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

pub mod rotation;
pub use rotation::RotatingJournal;

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
    size: usize,
}

unsafe impl Send for MmapJournal {}
unsafe impl Sync for MmapJournal {}

impl MmapJournal {
    /// Ouvre/cree le segment journal en mmap. Renvoie `Err` (jamais de panique)
    /// si le chemin est invalide ou si open/ftruncate/mmap echouent.
    pub fn new(file_path: &str) -> std::io::Result<Self> {
        Self::new_with_size(file_path, JOURNAL_SIZE)
    }

    /// Comme `new` mais avec une taille de segment explicite (rotation / tests).
    pub fn new_with_size(file_path: &str, size: usize) -> std::io::Result<Self> {
        let size = size.max(8);
        let c_path = CString::new(file_path)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        // SAFETY:
        // 1. `c_path` is a valid, NUL-terminated CString derived from a &str; `open()` dereferences
        //    it only for the duration of this call.
        // 2. `fd` is checked for < 0 before any further use; every error path calls `close(fd)`.
        // 3. `ftruncate` is only called on a valid fd and its failure is handled.
        // 4. `mmap` receives a valid size (> 0 after `.max(8)`), a valid fd (open + ftruncated),
        //    and returns either MAP_FAILED or a valid pointer; we check for MAP_FAILED before
        //    storing the result.
        // 5. The fd is closed immediately after mmap — MAP_SHARED keeps the mapping alive.
        // 6. No other thread can access this struct until `new` returns, so there is no data race
        //    on `mmap_ptr` or `write_offset`.
        // INVARIANTS: After this block, `mmap_ptr` points to a valid `size`-byte shared-memory
        // region (or the function returned Err). The fd must NOT be used after close.
        // FAILURE: If any syscall fails, we return `Err` with the OS error — no memory is leaked.
        unsafe {
            let fd = libc::open(c_path.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o666);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ftruncate(fd, size as libc::off_t) != 0 {
                let e = std::io::Error::last_os_error();
                libc::close(fd);
                return Err(e);
            }
            let mmap_ptr = libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            libc::close(fd);
            if mmap_ptr == libc::MAP_FAILED {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self {
                mmap_ptr: mmap_ptr as *mut u8,
                write_offset: AtomicUsize::new(0),
                size,
            })
        }
    }

    /// PROTOCOLE DE PUBLICATION : reserve le slot (CAS), ecrit tag+payload, PUIS
    /// publie `size` en DERNIER via store Release. Lecteur Acquire : size>0 =>
    /// tag+payload visibles (happens-before), aucune lecture dechiree. size==0 =
    /// marqueur "non commite" -> payload vide refuse.
    pub fn append_log(&self, tag: u32, data: &[u8]) -> bool {
        let size = data.len();
        if size == 0 || size > u32::MAX as usize {
            return false;
        }
        let need = padded_len(size);
        loop {
            let current_offset = self.write_offset.load(Ordering::Acquire);
            if current_offset + need >= self.size {
                return false;
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
                // SAFETY:
                // 1. CAS succeeded, so this thread is the sole owner of the slot at
                //    `current_offset..current_offset+need` — no other writer can overlap.
                // 2. `current_offset + need <= self.size` is guaranteed by the bounds check above.
                // 3. `mmap_ptr.add(current_offset)` is within the mapped region.
                // 4. `copy_nonoverlapping` writes exactly 4 bytes (tag) and `size` bytes (payload)
                //    into non-overlapping regions: tag at base[0..4], size at base[4..8],
                //    payload at base[8..8+size].
                // 5. `size_cell` is properly aligned for AtomicU32 because `base.add(4)` falls
                //    within the mmap region (which is page-aligned) + offset that is a multiple of 4.
                // 6. The Release store on `size_cell` publishes the write — the reader's Acquire
                //    load of `size` establishes happens-before, preventing torn reads.
                // INVARIANT: The size field at base+4 is written LAST (after tag and payload),
                //    so a concurrent reader that sees size==0 knows the slot is uncommitted.
                // FAILURE: Violating any invariant would cause UB (unaligned access, data race,
                //    or torn reads seen by concurrent readers).
                unsafe {
                    let base = self.mmap_ptr.add(current_offset);
                    std::ptr::copy_nonoverlapping(&tag as *const u32 as *const u8, base, 4);
                    std::ptr::copy_nonoverlapping(data.as_ptr(), base.add(8), size);
                    let size_cell = base.add(4) as *const AtomicU32;
                    (*size_cell).store(size as u32, Ordering::Release);
                }
                return true;
            }
        }
    }

    /// Relit les records COMMITES (`size` lu en Acquire ; size==0 -> fin lisible).
    pub fn read_committed(&self) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        let mut off = 0usize;
        loop {
            if off + 8 > self.size {
                break;
            }
            // SAFETY:
            // 1. `off` is bounded: we check `off + 8 <= self.size` before entering this block.
            // 2. `base` = `mmap_ptr.add(off)` is within the mapped region.
            // 3. `size` is read via AtomicU32 with Acquire ordering — guarantees we see all
            //    writes (tag + payload) that happened-before the writer's Release store.
            // 4. We check `off + 8 + size <= self.size` before reading the payload — prevents
            //    out-of-bounds access.
            // 5. `copy_nonoverlapping` reads exactly 4 bytes (tag) and `size` bytes (payload)
            //    from contiguous, valid mmap memory. The source regions do not overlap with
            //    the destination (`tag_bytes` / `payload` on the stack/heap).
            // 6. If `size == 0` we break immediately — no invalid memory is accessed.
            // INVARIANT: A committed record always has a non-zero `size`, valid `tag`, and
            //    `off + 8 + size <= self.size`. Uncommitted slots have `size == 0` and are
            //    treated as end-of-data.
            // FAILURE: If a previous writer panicked mid-write, we could read a partially
            //    written `size` — but `size == 0` causes a clean break, and a non-zero partial
            //    value would only cause us to read the payload (possibly garbage) and advance
            //    `off` — no UB, just a stale record. The protocol (size written last) prevents
            //    this in practice.
            unsafe {
                let base = self.mmap_ptr.add(off);
                let size = (*(base.add(4) as *const AtomicU32)).load(Ordering::Acquire) as usize;
                if size == 0 {
                    break;
                }
                if off + 8 + size > self.size {
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
        // SAFETY:
        // 1. `self.mmap_ptr` is a valid pointer returned by mmap in `new_with_size`.
        // 2. `len` is read from `write_offset` (Acquire) and is <= `self.size` (the mapped
        //    region size). `len` is only the portion that has been written to, which is
        //    always within the mapped region.
        // 3. `MS_SYNC` causes the call to block until all data is written to disk.
        // INVARIANT: `len <= self.size` — the mmap region is exactly `self.size` bytes.
        // FAILURE: msync returns 0 on success; a non-zero return indicates a failure
        //    (e.g., the mapping was unmapped by another thread — impossible here since
        //    only `drop` calls munmap). Returns false on failure, no UB.
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
        // SAFETY:
        // 1. This is called exactly once during Drop — no other code accesses `mmap_ptr`
        //    after this point (single-threaded ownership at drop time).
        // 2. `self.write_offset.load(Acquire)` returns the last committed write offset.
        //    We msync only the written portion (`len`) so we don't sync beyond the mapping.
        // 3. `msync` with MS_SYNC flushes all dirty pages before `munmap` — guarantees
        //    durability before releasing the mapping.
        // 4. `munmap(self.mmap_ptr, self.size)` unmaps exactly the region originally
        //    created by mmap — the pointer and size match the original allocation.
        // INVARIANT: `mmap_ptr` is the exact pointer from mmap, and `self.size` is the
        //    exact size passed to mmap. Only called once (Drop semantics).
        // FAILURE: If msync fails, we proceed with munmap anyway (data may not be durable
        //    but the mapping is still valid to unmap). munmap cannot fail on a valid
        //    mapping. No memory leak — the virtual address range is reclaimed.
        unsafe {
            let len = self.write_offset.load(Ordering::Acquire);
            if len > 0 {
                libc::msync(self.mmap_ptr as *mut libc::c_void, len, libc::MS_SYNC);
            }
            libc::munmap(self.mmap_ptr as *mut libc::c_void, self.size);
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
            let size = u32::from_le_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]])
                as usize;
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
    fn new_renvoie_err_sans_paniquer() {
        let r = MmapJournal::new("/tmp/inva\0lide.bin"); // NUL interne -> CString echoue
        assert!(r.is_err(), "chemin invalide doit renvoyer Err");
        let r2 = MmapJournal::new("/nonexistent_dir_xyz_42/journal.bin"); // open ENOENT
        assert!(r2.is_err(), "repertoire inexistant doit renvoyer Err");
        println!("PREUVE no-panic : new() -> Err sur chemin invalide et repertoire inexistant");
    }

    #[test]
    fn sync_puis_reopen_intact() {
        let path = format!("/tmp/soul_journal_test_sync_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        {
            let j = MmapJournal::new(&path).expect("create journal");
            assert!(j.append_log(0xAA, b"hello"));
            assert!(j.append_log(0xBB, b"world!!"));
            assert!(j.sync());
            assert_eq!(
                relire_records(&path, 2),
                vec![(0xAAu32, b"hello".to_vec()), (0xBBu32, b"world!!".to_vec())]
            );
            println!("PREUVE sync+reopen : 2 records intacts");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn durabilite_sans_drop() {
        let path = format!("/tmp/soul_journal_test_nodrop_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = MmapJournal::new(&path).expect("create journal");
        assert!(j.append_log(0x01, b"durable-record"));
        assert!(j.sync());
        std::mem::forget(j);
        assert_eq!(
            relire_records(&path, 1),
            vec![(0x01u32, b"durable-record".to_vec())]
        );
        println!("PREUVE no-Drop : record present apres mem::forget");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_committed_et_payload_vide_refuse() {
        let path = format!("/tmp/soul_journal_test_rc_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = MmapJournal::new(&path).expect("create journal");
        assert!(j.append_log(10, b"abc"));
        assert!(j.append_log(20, b"defgh"));
        assert!(!j.append_log(30, b""), "payload vide refuse");
        assert_eq!(
            j.read_committed(),
            vec![(10u32, b"abc".to_vec()), (20u32, b"defgh".to_vec())]
        );
        println!("PREUVE read_committed + refus payload vide");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn flusher_periodique_et_auto_stop() {
        let path = format!("/tmp/soul_journal_test_flush_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = Arc::new(MmapJournal::new(&path).expect("create journal"));
        let h = j
            .spawn_flusher(Duration::from_millis(20))
            .expect("spawn flusher");
        assert!(j.append_log(0x77, b"flushed-by-thread"));
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(
            relire_records(&path, 1),
            vec![(0x77u32, b"flushed-by-thread".to_vec())]
        );
        drop(j);
        let start = std::time::Instant::now();
        h.join().expect("join flusher");
        assert!(start.elapsed() < Duration::from_secs(2));
        println!("PREUVE flusher + auto-stop en {:?}", start.elapsed());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn concurrent_aucune_lecture_dechiree() {
        let path = format!("/tmp/soul_journal_test_conc_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        let j = Arc::new(MmapJournal::new(&path).expect("create journal"));
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
                    assert_eq!(payload.len(), 16);
                    assert!(
                        payload.iter().all(|&b| b == expected),
                        "LECTURE DECHIREE tag={}",
                        tag
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
            assert_eq!(*tag, (i as u32) + 1);
            assert!(payload.iter().all(|&b| b == (tag % 251) as u8));
        }
        println!("PREUVE concurrent : {} records, aucune lecture dechiree", N);
        let _ = std::fs::remove_file(&path);
    }
}
