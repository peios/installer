//! Allocating a loop device, because nothing on a Peios image can.
//!
//! The installer has to mount the shipped `rootfs.squashfs` in order to
//! copy it, and `mount -o loop` cannot do it: the live system booted
//! from that same file, so a loop device is already bound to it, and
//! mount's own loop handling finds that device and reuses it. The
//! superblock on it is already mounted — in the initramfs's namespace,
//! which the booted system has no path to — so the second mount fails
//! with EBUSY from `fsconfig(create)` and there is no way to reach the
//! first.
//!
//! The ordinary answer is `losetup`, which Peios does not ship (checked
//! against the image: no losetup, and peiosutils has no equivalent), so
//! this asks the kernel directly. `/dev/loop-control` hands out a free
//! device; `LOOP_SET_FD` binds the file to it. Opening the backing file
//! read-only is what makes the device read-only.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

// libc's ioctl request type follows the target ABI (`c_ulong` for glibc,
// `c_int` for musl). Naming that type keeps this code correct for both the
// native test build and the musl distribution build.
const LOOP_SET_FD: libc::Ioctl = 0x4C00;
const LOOP_CLR_FD: libc::Ioctl = 0x4C01;
const LOOP_CTL_GET_FREE: libc::Ioctl = 0x4C82;

/// A loop device bound to a file, released on drop.
pub struct LoopDevice {
    path: PathBuf,
    device: File,
    /// Held so the binding cannot outlive the open file.
    _backing: File,
}

impl LoopDevice {
    /// Bind `backing` to a free loop device, read-only.
    pub fn attach_readonly(backing: &Path) -> io::Result<LoopDevice> {
        let control = File::open("/dev/loop-control")?;
        let n = unsafe { libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        let path = PathBuf::from(format!("/dev/loop{n}"));

        // devtmpfs materialises the node when the kernel creates the
        // device, with no udev involved — but the ioctl returns before
        // the node is necessarily visible, so opening it is retried
        // rather than assumed.
        let device = open_with_retry(&path)?;
        let file = File::open(backing)?;
        let rc = unsafe { libc::ioctl(device.as_raw_fd(), LOOP_SET_FD, file.as_raw_fd()) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(LoopDevice {
            path,
            device,
            _backing: file,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LoopDevice {
    fn drop(&mut self) {
        // Best effort: the filesystem on it must already be unmounted,
        // and if it is not the kernel refuses and the device is cleared
        // when the last reference goes anyway.
        unsafe {
            libc::ioctl(self.device.as_raw_fd(), LOOP_CLR_FD);
        }
    }
}

fn open_with_retry(path: &Path) -> io::Result<File> {
    let mut last = None;
    for _ in 0..50 {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(f) => return Ok(f),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("loop device never appeared")))
}
