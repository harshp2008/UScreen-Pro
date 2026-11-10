/*
 * helper_state.h: the state the helper threads share, and its tunables.
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It owns the globals (EVDI handle, the framebuffer, the triple buffer and its dirty masks, the timing counters) and the constants; it owns no functions except cmp_int.
 */

static evdi_handle g_handle = EVDI_INVALID_HANDLE;
/* Index of the EVDI card in use (/dev/dri/cardN), -1 before it is known. */
static int g_device_index = -1;
/* Cleared by a stop signal to end every loop. */
static volatile int g_running = 1;

/* File descriptor of the capture FIFO, -1 while it is closed. */
static int g_capture_fifo_fd = -1;
/* Path of the capture FIFO given with --capture-fifo. */
static const char *g_fifo_path = NULL;
/* Target frames per second (--fps, 60 by default). */
static int g_fps = 60;

/* EVDI-registered framebuffer (stride-padded, written by the kernel) */
static unsigned char *g_framebuffer = NULL;
/* Size in bytes of the framebuffer (stride x height). */
static int g_fb_size = 0;
/* Mode width in pixels. */
static int g_mode_w = 0;
/* Mode height in pixels. */
static int g_mode_h = 0;
/* Bytes per pixel of the mode. */
static int g_mode_bpp = 4;
/* Bytes per framebuffer row, padded to 64. */
static int g_mode_stride = 0;
/* Set once the first mode has been received and the buffers are ready. */
static volatile int g_have_mode = 0;
/* Card number forced with --card, -1 to search. */
static int g_pin_card = -1;
/* Whether the display is powered on (from the DPMS event). */
static int g_dpms_on = 0;

/* Triple buffer for FIFO writes: grabber packs into g_fill, swaps with
   g_latest; writer swaps g_latest into g_write and streams it out.
   Pointer swaps only — no copies between threads, no stalls. */
static pthread_mutex_t g_swap_mutex = PTHREAD_MUTEX_INITIALIZER;
/* Signalled by publish_frame() so the writer wakes the moment a frame exists
   rather than on its own timer — see writer_thread(). */
/* Initialised in main() against CLOCK_MONOTONIC.
   NOT PTHREAD_COND_INITIALIZER: that condvar measures absolute timeouts
   against CLOCK_REALTIME, while the deadlines here come from
   CLOCK_MONOTONIC. Monotonic time is seconds-since-boot and realtime is
   seconds-since-1970, so every deadline looked decades overdue,
   pthread_cond_timedwait returned ETIMEDOUT immediately and the writer
   thread spun at ~65% of a core doing nothing. */
static pthread_cond_t g_frame_ready;
/* Buffer being converted into. */
static unsigned char *g_fill = NULL;
/* Newest converted frame, waiting for the writer. */
static unsigned char *g_latest = NULL;
/* Frame being written to the FIFO. */
static unsigned char *g_write = NULL;
/* Whether g_latest holds a frame the writer has not taken. */
static volatile int g_latest_valid = 0;

/* Per-buffer "which chroma rows are out of date in THIS buffer", one bit per
   chroma row. Converting the whole frame every time is wasted work: a desktop
   typically changes a small band, and EVDI already tells us which.
   Why per buffer rather than one global list: with triple buffering each
   buffer was last filled at a different moment, so each is stale in a
   different set of rows. A single shared list would leave whichever buffer
   was skipped holding torn, half-updated content. Damage is therefore
   recorded into all three, and cleared only for the buffer just converted.
   The masks swap together with the buffer pointers they describe. */
static unsigned char *g_dirty_fill = NULL;
/* Stale-row mask belonging to g_latest. */
static unsigned char *g_dirty_latest = NULL;
/* Stale-row mask belonging to g_write. */
static unsigned char *g_dirty_write = NULL;
/* Size of one mask in bytes (one bit per chroma row). */
static int g_dirty_bytes = 0; /* size of one mask */
static int g_chroma_rows = 0; /* mode height / 2 */
static int g_packed_size = 0; /* NV12 frame: out_w*out_h*3/2 */

/* Integer downscale applied on the way to the encoder. The desktop keeps its
   native mode — window layout and scaling are unaffected — while the stream
   carries fewer pixels. That cuts both the bytes crossing into the encoder and
   the tablet's decode time, which measurements put at roughly 7-8ms fixed plus
   1.2ms per megapixel. An integer divisor means the tablet upscales by a whole
   number, avoiding resampling artefacts on top of the softness.
   1 = native, and keeps the original tight conversion loop. */
static int g_scale = 1;
/* Width of the NV12 frame (source width divided by the scale, even). */
static int g_out_w = 0;
/* Height of the NV12 frame. */
static int g_out_h = 0;
/* Set when all buffers are allocated and frames can be published. */
static volatile int g_buffers_ready = 0;

/* How long an unchanged screen may go without a frame being re-sent.
   This is not just about the client's read timeout. The encoder's keyframe
   interval (-g) counts frames, not seconds, so the slower we feed it while
   idle, the longer the wall-clock gap between IDRs — and a client that joins
   or recovers from a drop cannot start decoding until one arrives. At 200ms
   the idle floor is 5fps, which keeps that gap bounded at a few seconds while
   still cutting idle work by an order of magnitude.
   The real fix is requesting an IDR on demand, which needs the in-process
   encoder rather than a pipe into the ffmpeg CLI. */
/* At 33ms (~30fps) the idle floor keeps the encoder and the client's read
   timeout smoothly paced without freezing, while identical frames consume
   negligible bitrate in hardware encoders via skip macroblocks.
   In Max Performance Mode, this is lowered to 16ms (approx 60 FPS). */
#define DEFAULT_IDLE_KEEPALIVE_MS 33
/* Longest gap between FIFO writes of an unchanged screen, in ms (33, or 16 with --max-performance). */
static int g_idle_keepalive_ms = DEFAULT_IDLE_KEEPALIVE_MS;

/* Flush duplicate (RCA 56 s10). ffmpeg + VAAPI hold every frame until the NEXT
   one is written to the FIFO, so an isolated update only reaches the tablet
   when the next frame (the idle keepalive, 33 ms later, or the next damage)
   pushes it out: measured p50 119 ms for updates 100 ms apart, 31 ms when the
   same frame is simply written a second time 2 ms later. After a fresh frame
   that follows a quiet period, write one duplicate shortly after. During a
   burst (frames closer than FLUSH_SPARSE_US) nothing changes, so continuous
   motion does not double the encoder load. USCREEN_FLUSH_DUP=0 turns it off. */
#define FLUSH_SPARSE_US 25000LL
/* Delay of the duplicate write after an isolated fresh frame (2 ms). */
#define FLUSH_DELAY_NS 2000000L
/* Whether flush duplicates are enabled (USCREEN_FLUSH_DUP=0 turns them off). */
static int g_flush_enabled = 1;
/* A duplicate of the last frame is due. */
static int g_flush_pending = 0;
/* Microsecond time of the last fresh frame written. */
static long long g_last_fresh_write_us = 0;
/*
 * Counters for the statistics line: flush duplicates armed, sent, fresh frames sent, and flushes blocked by the cursor-exit guard.
 */
static volatile long long g_flush_armed_n = 0, g_flush_sent_n = 0, g_fresh_sent_n = 0, g_flush_blocked_n = 0;
/* Millisecond time of the last write to the FIFO. */
static long long g_last_write_ms = 0;

/* Forward declaration (defined in helper_timing.h). */
static long long now_us(void);
/* Inter-process shared memory ring for microsecond stage timestamp tracking */
#define TIMING_RING_SIZE 1024
/* Magic number of the shared timing ring ('USCR'). */
#define TIMING_MAGIC 0x55534352 /* 'USCR' */
#define TIMING_VERSION 1

struct FrameTimingRecord {
  uint32_t seq;
  uint32_t is_fresh;
  uint64_t t_request_us;    /* Mutter frame request */
  uint64_t t_ready_us;      /* Stage a: EVDI receives UPDATE_READY */
  uint64_t t_grab_done_us;  /* Stage b: evdi_grab_pixels completes */
  uint64_t t_conv_done_us;  /* Stage b': NV12 conversion complete */
  uint64_t t_fifo_write_us; /* Stage c: FIFO write completes (FFmpeg receives
                               frame) */
};

struct SharedTimingRing {
  uint32_t magic;
  uint32_t version;
  struct FrameTimingRecord records[TIMING_RING_SIZE];
};

/* Path of the timing ring file (--timing-shm), or NULL for none. */
static const char *g_timing_shm_path = NULL;
/* The mapped timing ring, or NULL. */
static struct SharedTimingRing *g_timing_shm = NULL;
/* File descriptor of the timing ring file. */
static int g_timing_shm_fd = -1;
/* Sequence number of the next FIFO write. */
static uint32_t g_fifo_seq = 0;

/* When the last update-ready event arrived (µs). */
static long long g_ready_us = 0;
/* When the last conversion finished (µs). */
static long long g_conv_us = 0;

/* Request time of the frame in g_latest. */
static long long g_latest_req_us = 0;
/* Update-ready time of the frame in g_latest. */
static long long g_latest_ready_us = 0;
/* Conversion-done time of the frame in g_latest. */
static long long g_latest_conv_us = 0;

/* Request time of the frame in g_write. */
static long long g_write_req_us = 0;
/* Update-ready time of the frame in g_write. */
static long long g_write_ready_us = 0;
/* Conversion-done time of the frame in g_write. */
static long long g_write_conv_us = 0;

/* Sum of conversion durations in this statistics window (µs). */
static long long g_conv_sum_us = 0;
/* Number of conversions in the window. */
static int g_conv_n = 0;
/* Sum of FIFO write durations in the window (µs). */
static long long g_write_sum_us = 0;
/* Number of FIFO writes in the window. */
static int g_write_n = 0;

/* Capture-side latency: grab → NV12 → handed to the encoder's FIFO.
   Reported as percentiles so the cost of the pipe-to-ffmpeg design can be
   compared against the decode and wire costs measured on the other side. */
#define LAT_SAMPLES 256
/* Capture-to-FIFO latency samples of the window (µs). */
static int g_lat_us[LAT_SAMPLES];
/* Number of samples in g_lat_us. */
static int g_lat_count = 0;
static long long g_grab_us = 0; /* when the frame now in g_fill was grabbed */
static long long g_latest_grab_us = 0;
/* When the frame in g_write was grabbed (µs). */
static long long g_write_grab_us = 0;

/* qsort comparator for ints, ascending. */
static int cmp_int(const void *a, const void *b) {
  int x = *(const int *)a, y = *(const int *)b;
  return (x > y) - (x < y);
}

static volatile int g_update_pending =
    0; /* request_update sent, waiting for update_ready */
static volatile int g_writer_busy =
    0; /* writer is streaming g_write to the FIFO */
static volatile int g_mode_generation = 0; /* bumped on every mode change */
static volatile long long g_grab_count = 0;

/* Where the capture cycle spends its time, printed with the 5s stats. The
   two halves of the cycle are the wait for the compositor to answer a
   request and the copy out of the framebuffer; knowing both is what tells
   a slow compositor from a slow helper. */
static long long g_req_us = 0;      /* when the outstanding request was sent */
static long long g_wait_sum_us = 0; /* request → update_ready */
static int g_wait_n = 0;
static long long g_grab_sum_us = 0; /* evdi_grab_pixels duration */
static int g_grab_n = 0;
static int g_immediate_n = 0; /* requests the kernel answered at once */
static int g_empty_n = 0;     /* grabs that returned no damaged rects */
static volatile sig_atomic_t g_throttled = 0;
/* Forward declaration (defined in helper_convert.h). */
static void mark_all_dirty(void);
