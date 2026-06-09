//! Shared memory primitives — memfd_create + mmap for zero-copy IPC.

use anyhow::{Context, Result};
use nix::sys::mman::{mmap, munmap, MapFlags, ProtFlags};
use nix::sys::memfd::MemFdCreateFlag;
use nix::unistd::ftruncate;
use std::num::NonZeroUsize;
use std::os::unix::io::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};

/// A shared memory region backed by a memfd.
pub struct ShmRegion {
    _fd: OwnedFd,
    ptr: *mut u8,
    len: usize,
}

unsafe impl Send for ShmRegion {}
unsafe impl Sync for ShmRegion {}

impl ShmRegion {
    /// Create a new anonymous shared memory region via memfd_create.
    pub fn create(name: &str, len: usize) -> Result<Self> {
        use std::ffi::CString;
        let c_name = CString::new(name).context("invalid memfd name")?;
        let fd: OwnedFd = nix::sys::memfd::memfd_create(&c_name, MemFdCreateFlag::MFD_CLOEXEC)
            .context("memfd_create failed")?;

        ftruncate(&fd, len as i64).context("ftruncate failed")?;

        let non_zero_len = NonZeroUsize::new(len).context("shm region size must be > 0")?;

        let ptr = unsafe {
            mmap(
                None,
                non_zero_len,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                &fd,
                0,
            )
            .context("mmap failed")?
        };

        let ptr = ptr.as_ptr() as *mut u8;

        // Zero the region
        unsafe {
            std::ptr::write_bytes(ptr, 0, len);
        }

        Ok(Self { _fd: fd, ptr, len })
    }

    /// Open an existing memfd by passing its fd (e.g., via UDS SCM_RIGHTS).
    pub fn from_fd(fd: OwnedFd, len: usize) -> Result<Self> {
        let non_zero_len = NonZeroUsize::new(len).context("shm region size must be > 0")?;

        let ptr = unsafe {
            mmap(
                None,
                non_zero_len,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                &fd,
                0,
            )
            .context("mmap from fd failed")?
        };

        let ptr = ptr.as_ptr() as *mut u8;

        Ok(Self { _fd: fd, ptr, len })
    }

    /// Get a raw pointer to the region.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Get a mutable pointer to the region.
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Get the length of the region.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Get the raw fd (for passing via UDS SCM_RIGHTS).
    pub fn raw_fd(&self) -> RawFd {
        self._fd.as_raw_fd()
    }

    /// Get a borrowed fd reference.
    pub fn fd(&self) -> BorrowedFd<'_> {
        self._fd.as_fd()
    }

    /// Write bytes into the region at a given offset.
    pub fn write_bytes(&self, offset: usize, data: &[u8]) -> Result<()> {
        if offset + data.len() > self.len {
            anyhow::bail!(
                "write out of bounds: offset={} len={} region_len={}",
                offset,
                data.len(),
                self.len
            );
        }
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(offset), data.len());
        }
        Ok(())
    }

    /// Read bytes from the region at a given offset.
    pub fn read_bytes(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        if offset + len > self.len {
            anyhow::bail!(
                "read out of bounds: offset={} len={} region_len={}",
                offset,
                len,
                self.len
            );
        }
        let mut buf = vec![0u8; len];
        unsafe {
            std::ptr::copy_nonoverlapping(self.ptr.add(offset), buf.as_mut_ptr(), len);
        }
        Ok(buf)
    }
}

impl Drop for ShmRegion {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(
                std::ptr::NonNull::new(self.ptr as *mut std::ffi::c_void).unwrap(),
                self.len,
            );
        }
    }
}

/// Send a file descriptor over a Unix Domain Socket.
pub fn send_fd(sockfd: RawFd, fd: RawFd) -> Result<()> {
    use nix::sys::socket::{sendmsg, ControlMessage, MsgFlags};
    use std::io::IoSlice;

    let iov = [IoSlice::new(b"")];
    let fds = [fd];
    let cmsg = ControlMessage::ScmRights(&fds);

    sendmsg::<nix::sys::socket::UnixAddr>(sockfd, &iov, &[cmsg], MsgFlags::empty(), None)
        .context("sendmsg SCM_RIGHTS failed")?;
    Ok(())
}

/// Receive a file descriptor from a Unix Domain Socket.
pub fn recv_fd(sockfd: RawFd) -> Result<OwnedFd> {
    use nix::sys::socket::{recvmsg, MsgFlags};
    use std::io::IoSliceMut;

    let mut buf = [0u8; 1];
    let mut iov = [IoSliceMut::new(&mut buf)];
    let mut cmsg_buffer = nix::cmsg_space!([RawFd; 1]);

    let msg = recvmsg::<nix::sys::socket::UnixAddr>(sockfd, &mut iov, Some(&mut cmsg_buffer), MsgFlags::empty())
        .context("recvmsg failed")?;

    let cmsgs = msg.cmsgs().context("failed to parse cmsgs")?;
    for cmsg in cmsgs {
        if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = cmsg {
            if let Some(&fd) = fds.first() {
                let owned = unsafe { OwnedFd::from_raw_fd(fd) };
                return Ok(owned);
            }
        }
    }

    anyhow::bail!("no fd received in SCM_RIGHTS")
}
