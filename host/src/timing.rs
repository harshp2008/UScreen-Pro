use std::fs::OpenOptions;
use std::path::Path;

pub const TIMING_RING_SIZE: usize = 1024;
pub const TIMING_MAGIC: u32 = 0x55534352; // 'USCR'
pub const TIMING_VERSION: u32 = 1;

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

#[repr(C)]
pub struct SharedTimingRing {
    pub magic: u32,
    pub version: u32,
    pub records: [FrameTimingRecord; TIMING_RING_SIZE],
}

pub struct TimingReader {
    mmap_ptr: *const SharedTimingRing,
    len: usize,
}

unsafe impl Send for TimingReader {}
unsafe impl Sync for TimingReader {}

impl TimingReader {
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
    fn drop(&mut self) {
        if !self.mmap_ptr.is_null() && self.mmap_ptr != libc::MAP_FAILED as *const _ {
            unsafe {
                libc::munmap(self.mmap_ptr as *mut libc::c_void, self.len);
            }
        }
    }
}

pub fn now_monotonic_us() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64) * 1_000_000 + (ts.tv_nsec as u64) / 1_000
}
