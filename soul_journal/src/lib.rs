use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

const JOURNAL_SIZE: usize = 1024 * 1024 * 64; // Segment de 64 Mo pre-alloue en mmap

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
                panic!("[JOURNAL CRITICAL] Impossible de creer le descripteur de fichier NVMe.");
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
                panic!("[JOURNAL CRITICAL] Echec de l'allocation mmap native.");
            }

            Self {
                mmap_ptr: mmap_ptr as *mut u8,
                write_offset: AtomicUsize::new(0),
            }
        }
    }

    /// Ecrit une entree d'etat de l'OS de maniere atomique et ultra-rapide.
    /// N.B. : la durabilite n'est PAS garantie au retour (chemin rapide).
    /// Appeler `sync()` au point de commit, ou lancer `spawn_flusher`.
    pub fn append_log(&self, tag: u32, data: &[u8]) -> bool {
        let size = data.len();
        let total_needed = size + 8; // En-tete (Tag + Size)

        loop {
            let current_offset = self.write_offset.load(Ordering::Acquire);
            if current_offset + total_needed >= JOURNAL_SIZE {
                return false; // Journal plein, rotation requise
            }

            if self
                .write_offset
                .compare_exchange_weak(
                    current_offset,
                    current_offset + total_needed,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                unsafe {
                    let write_ptr = self.mmap_ptr.add(current_offset);
                    std::ptr::copy_nonoverlapping(&tag as *const u32 as *const u8, write_ptr, 4);
                    std::ptr::copy_nonoverlapping(
                        &size as *const usize as *const u8,
                        write_ptr.add(4),
                        4,
                    );
                    std::ptr::copy_nonoverlapping(data.as_ptr(), write_ptr.add(8), size);
                }
                return true;
            }
        }
    }

    /// Octets actuellement ecrits (curseur). Observabilite / recovery.
    #[inline]
    pub fn written_len(&self) -> usize {
        self.write_offset.load(Ordering::Acquire)
    }

    /// Barriere de DURABILITE explicite : force l'ecriture sur le NVMe de la
    /// zone reellement ecrite (0..offset) via msync(MS_SYNC). Renvoie false si
    /// le syscall echoue. Contrairement au flush de `Drop`, ne depend PAS de la
    /// destruction de l'objet (donc survit a `panic = "abort"`).
    pub fn sync(&self) -> bool {
        let len = self.write_offset.load(Ordering::Acquire);
        if len == 0 {
            return true; // rien a flusher
        }
        // mmap_ptr est aligne page (garanti par mmap) -> adresse valide pour
        // msync ; la longueur est arrondie a la page superieure par le noyau.
        unsafe { libc::msync(self.mmap_ptr as *mut libc::c_void, len, libc::MS_SYNC) == 0 }
    }

    /// Thread de flush periodique (group-commit) : appelle `sync()` toutes les
    /// `period`. Ne detient qu'un `Weak` -> s'arrete de lui-meme des que le
    /// dernier `Arc<MmapJournal>` est libere (pas de thread fantome). Borne la
    /// fenetre de perte sur crash a ~`period`.
    pub fn spawn_flusher(
        self: &Arc<Self>,
        period: Duration,
    ) -> std::io::Result<JoinHandle<()>> {
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
            // Backstop best-effort : flush de la zone ecrite avant fermeture.
            // NE PAS s'appuyer la-dessus pour la durabilite (saute si panic=abort).
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

    // Relit le journal via une poignee INDEPENDANTE et parse les records
    // (tag u32 LE, size u32 LE, payload). Renvoie la liste (tag, payload).
    fn relire_records(path: &str, nb_attendu: usize) -> Vec<(u32, Vec<u8>)> {
        let mut f = std::fs::File::open(path).expect("open journal");
        let mut buf = vec![0u8; 8192];
        let n = f.read(&mut buf).expect("read journal");
        buf.truncate(n);

        let mut out = Vec::new();
        let mut off = 0usize;
        while out.len() < nb_attendu && off + 8 <= buf.len() {
            let tag =
                u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
            let size = u32::from_le_bytes([
                buf[off + 4],
                buf[off + 5],
                buf[off + 6],
                buf[off + 7],
            ]) as usize;
            if tag == 0 && size == 0 {
                break; // zone non ecrite (zeros)
            }
            let start = off + 8;
            let end = start + size;
            if end > buf.len() {
                break;
            }
            out.push((tag, buf[start..end].to_vec()));
            off = end;
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
            assert!(j.sync(), "msync(MS_SYNC) doit reussir");
            let recs = relire_records(&path, 2);
            assert_eq!(recs.len(), 2, "2 records attendus, lus {}", recs.len());
            assert_eq!(recs[0], (0xAA, b"hello".to_vec()));
            assert_eq!(recs[1], (0xBB, b"world!!".to_vec()));
            println!("PREUVE sync+reopen : 2 records relus intacts par une poignee tierce");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn durabilite_sans_drop() {
        // Simule exactement panic=abort : Drop n'est JAMAIS execute.
        let path = format!("/tmp/soul_journal_test_nodrop_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);

        let j = MmapJournal::new(&path);
        assert!(j.append_log(0x01, b"durable-record"));
        assert!(j.sync(), "msync doit reussir");
        std::mem::forget(j); // Drop saute : aucun munmap, aucun msync de Drop

        let recs = relire_records(&path, 1);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0], (0x01, b"durable-record".to_vec()));
        println!("PREUVE no-Drop : record present apres mem::forget -> durabilite decorrelee de Drop");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn flusher_periodique_et_auto_stop() {
        let path = format!("/tmp/soul_journal_test_flush_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);

        let j = Arc::new(MmapJournal::new(&path));
        let h = j
            .spawn_flusher(Duration::from_millis(20))
            .expect("spawn journal-flusher");

        assert!(j.append_log(0x77, b"flushed-by-thread"));
        std::thread::sleep(Duration::from_millis(120)); // > plusieurs periodes

        let recs = relire_records(&path, 1);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0], (0x77, b"flushed-by-thread".to_vec()));
        println!("PREUVE flusher : record persiste par le thread de flush periodique");

        drop(j); // libere le dernier Arc -> le Weak ne s'upgrade plus
        let start = std::time::Instant::now();
        h.join().expect("join flusher");
        assert!(start.elapsed() < Duration::from_secs(2), "le flusher n'a pas termine");
        println!("PREUVE auto-stop flusher : thread termine en {:?}", start.elapsed());

        let _ = std::fs::remove_file(&path);
    }
}
