//! Shared-memory ring of per-frame stage timestamps written by `evdi_helper` and read by the
//! host, so the latency report can split a frame's journey into grab / convert / queue / encode.
//! The layout is `#[repr(C)]` and must match `host/evdi/evdi_helper.c`.

use std::fs::OpenOptions;
use std::path::Path;

/// Number of slots in the shared timing ring; must match `evdi_helper.c`.
pub const TIMING_RING_SIZE: usize = 1024;
/// Magic number the helper writes once the ring is initialised.
pub const TIMING_MAGIC: u32 = 0x55534352; // 'USCR'
                                          // The C side also writes a layout version (`TIMING_VERSION`, currently 1) into the ring header;
                                          // the host does not check it, so it is not duplicated here. Bump it in `evdi_helper.c` and the struct together.

/// Stage timestamps of one frame (monotonic microseconds), written by the helper.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct FrameTimingRecord {
    pub seq: u32,
    pub is_fresh: u32,
    pub t_request_us: u64,
    pub t_ready_us: u64,
    pub t_grab_done_us: u64,
    pub t_conv_done_us: u64,
    pub t_fifo_write_us: u64,
}

/// Memory layout of the mmapped timing file; `#[repr(C)]` because the C helper writes it.
#[repr(C)]
pub struct SharedTimingRing {
    pub magic: u32,
    pub version: u32,
    pub records: [FrameTimingRecord; TIMING_RING_SIZE],
}

/// Read-only mapping of the helper's timing ring.
pub struct TimingReader {
    mmap_ptr: *const SharedTimingRing,
    len: usize,
}

unsafe impl Send for TimingReader {}
unsafe impl Sync for TimingReader {}

impl TimingReader {
    /// Create/size the file if needed and map it read-only; `None` if the mapping fails.
    pub fn open(path: &Path) -> Option<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .ok()?;
        let len = std::mem::size_of::<SharedTimingRing>();
        let _ = file.set_len(len as u64);
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            None
        } else {
            let ring = ptr as *const SharedTimingRing;
            Some(Self {
                mmap_ptr: ring,
                len,
            })
        }
    }

    /// Look up the record for frame `seq`, or `None` if the helper has not written it (or already overwrote it).
    pub fn get_record(&self, seq: u32) -> Option<FrameTimingRecord> {
        if self.mmap_ptr.is_null() {
            return None;
        }
        unsafe {
            let ring = &*self.mmap_ptr;
            if ring.magic != TIMING_MAGIC {
                return None;
            }
            let idx = (seq % (TIMING_RING_SIZE as u32)) as usize;
            let rec = ring.records[idx];
            if rec.seq == seq {
                Some(rec)
            } else {
                // Check all entries if there is an index offset
                for offset in 0..TIMING_RING_SIZE {
                    if ring.records[offset].seq == seq {
                        return Some(ring.records[offset]);
                    }
                }
                None
            }
        }
    }
}

impl Drop for TimingReader {
    /// Unmaps the shared timing memory.
    fn drop(&mut self) {
        if !self.mmap_ptr.is_null() && self.mmap_ptr != libc::MAP_FAILED as *const _ {
            unsafe {
                libc::munmap(self.mmap_ptr as *mut libc::c_void, self.len);
            }
        }
    }
}

/// `CLOCK_MONOTONIC` in microseconds — the clock the helper stamps records with, so differences are comparable.
pub fn now_monotonic_us() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec as u64) / 1_000
}
