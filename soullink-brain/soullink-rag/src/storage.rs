use memmap2::Mmap;
use std::fs::File;
use std::io::{Result, Write, BufWriter};
use std::os::unix::io::AsRawFd;

pub struct VectorStorage {
    buffer: Vec<f32>,
    max_buffer_size: usize,
    mmap: Option<Mmap>,
    file_path: String,
}

impl VectorStorage {
    pub fn new(file_path: &str, max_buffer_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_buffer_size),
            max_buffer_size,
            mmap: None,
            file_path: file_path.to_string(),
        }
    }

    pub fn append(&mut self, vector: &[f32]) -> Result<()> {
        self.buffer.extend_from_slice(vector);
        if self.buffer.len() >= self.max_buffer_size {
            self.flush()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let file = File::options().append(true).create(true).open(&self.file_path)?;
        let mut writer = BufWriter::new(file);

        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                self.buffer.as_ptr() as *const u8,
                self.buffer.len() * std::mem::size_of::<f32>(),
            )
        };
        writer.write_all(bytes)?;
        writer.flush()?;

        self.buffer.clear();
        self.remap()?;
        Ok(())
    }

    pub fn remap(&mut self) -> Result<()> {
        let file = File::open(&self.file_path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        let ret = unsafe {
            libc::mlock(mmap.as_ptr() as *const libc::c_void, mmap.len())
        };
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }

        self.mmap = Some(mmap);
        Ok(())
    }
}

impl Drop for VectorStorage {
    fn drop(&mut self) {
        if let Some(mmap) = &self.mmap {
            unsafe {
                libc::munlock(mmap.as_ptr() as *const libc::c_void, mmap.len());
            }
        }
    }
}
