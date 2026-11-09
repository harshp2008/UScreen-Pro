/*
 * helper_convert.h: BGRA to NV12 conversion (optionally downscaled), the conversion thread pool and per-buffer damage tracking.
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It owns pixel maths and the dirty masks; it does not talk to EVDI or the FIFO.
 */

/* --- BGRA -> NV12 conversion -------------------------------------------
   The kernel hands us a 32-bit BGRA framebuffer (21.9 MB at 2960x1848).
   Feeding that raw to the encoder is memory-bandwidth bound: it gets copied
   three times per frame (pack + pipe write + pipe read), which caps the
   pipeline around 70 fps even though NVENC itself sits near-idle. NV12 is
   1.5 bytes/px (8.2 MB) — 2.7x less data through every stage — and NVENC
   accepts it natively, so the encoder no longer color-converts either.

   Conversion uses BT.709 limited-range coefficients (matching the bt709 and
   `-color_range tv` tags the encoder writes) and is split across a small
   thread pool so it costs ~1 ms rather than stalling the capture loop.

   Full range was tried and reverted. It is theoretically better — 256 levels
   instead of 220 — but this tablet's decoder does not act on the SPS
   full-range flag even when the format also declares COLOR_RANGE_FULL: it
   assumes limited range, stretches the levels downwards and visibly crushes
   shadows. Limited range costs precision, not range (the decoder expands
   16-235 back to 0-255), so the only real exposure is slight banding in
   gradients, which is the lesser problem by a wide margin. */

/* Byte offsets of the red, green and blue samples inside one 32-bit pixel.
   The framebuffer is not always BGRA: the compositor picks the DRM format and
   libevdi reports it in the mode event. ARGB8888/XRGB8888 are B,G,R,x in
   memory (the default); ABGR8888/XBGR8888 are R,G,B,x. Reading the second as
   the first swaps red and blue — a blue desktop arrives orange on the tablet.
   Only changed from the mode callback, which runs on the thread that also
   dispatches conversions, so the workers never see it change under them. */
static int g_off_r = 2, g_off_g = 1, g_off_b = 0;

/* Packs four characters into a DRM fourcc code. */
#define FOURCC(a, b, c, d)                                                     \
  ((unsigned)(a) | ((unsigned)(b) << 8) | ((unsigned)(c) << 16) |              \
   ((unsigned)(d) << 24))

/*
 * Sets the byte offsets of red, green and blue from the DRM pixel format reported by libevdi (B,G,R,x is the default; R,G,B,x and the two 4-byte reversals are recognised); an unknown non-zero format is logged and treated as BGRA.
 */
static void set_pixel_order(unsigned int fmt) {
  int r = 2, g = 1, b = 0, known = 1;
  switch (fmt) {
  case FOURCC('A', 'R', '2', '4'):
  case FOURCC('X', 'R', '2', '4'): /* B,G,R,x */
    break;
  case FOURCC('A', 'B', '2', '4'):
  case FOURCC('X', 'B', '2', '4'): /* R,G,B,x */
    r = 0;
    g = 1;
    b = 2;
    break;
  case FOURCC('R', 'A', '2', '4'):
  case FOURCC('R', 'X', '2', '4'): /* x,B,G,R */
    r = 3;
    g = 2;
    b = 1;
    break;
  case FOURCC('B', 'A', '2', '4'):
  case FOURCC('B', 'X', '2', '4'): /* x,R,G,B */
    r = 1;
    g = 2;
    b = 3;
    break;
  default:
    known = 0; /* 0 = not reported by an old libevdi: keep BGRA */
    break;
  }
  if (g_off_r != r || g_off_b != b)
    fprintf(
        stderr,
        "[evdi-helper] Pixel format 0x%x: red at byte %d, blue at byte %d\n",
        fmt, r, b);
  if (!known && fmt != 0)
    fprintf(stderr, "[evdi-helper] Unknown pixel format 0x%x — assuming BGRA\n",
            fmt);
  g_off_r = r;
  g_off_g = g;
  g_off_b = b;
}

typedef struct {
  const unsigned char *src;   /* 32-bit pixels, stride-padded; see g_off_* */
  unsigned char *ydst;        /* Y plane, w bytes/row */
  unsigned char *uvdst;       /* interleaved CbCr, w bytes per chroma row */
  int w, h, stride;           /* source dimensions */
  int ow, oh;                 /* destination dimensions (w/scale, h/scale) */
  int scale;                  /* 1 = no downscale */
  int cy0, cy1;               /* chroma-row range [cy0, cy1) this job owns */
  const unsigned char *dirty; /* NULL = convert everything */
} conv_job_t;

/* Whether chroma row [cy] is marked stale in [mask]; a NULL mask means every row is. */
static inline int row_is_dirty(const unsigned char *mask, int cy) {
  return mask == NULL || (mask[cy >> 3] & (1u << (cy & 7)));
}

/* Downscaling variant: every output pixel is the mean of a scale x scale
   source block, and each chroma sample the mean of the 2*scale square it
   covers. Box averaging rather than point sampling — dropping pixels would
   alias hard on desktop content, where single-pixel lines are everywhere. */
/* Mean (b,g,r) of every output pixel of source rows [src_y0, src_y0 + n).

   Two colour channels are summed per 32-bit add: a pixel is split into its
   even bytes (v & 0x00FF00FF) and odd bytes ((v >> 8) & 0x00FF00FF), each
   pair sitting in its own 16-bit lane, so a whole n x n block (n <= 4, at
   most 16 x 255) is summed without a lane overflowing. Loading and
   zero-extending single bytes one channel at a time was what made the scaled
   path four times slower per pixel than the unscaled one. The block size is a
   compile-time constant in the common cases, so the inner loop unrolls and
   the division by n*n becomes a shift or a multiply. */
typedef uint32_t __attribute__((may_alias)) u32a;
/* Mask that selects the two even or the two odd bytes of a pixel in 16-bit lanes. */
#define SWAR_LANES 0x00FF00FFu

/*
 * Sums an N x N block of pixels per output pixel into the lane accumulators e (even bytes) and o (odd bytes), for N a compile-time constant so the loop unrolls.
 */
#define SWAR_ROWS(N)                                                           \
  for (int dy = 0; dy < (N); dy++) {                                           \
    const u32a *row = (const u32a *)(src + (size_t)(src_y0 + dy) * stride);    \
    for (int ox = 0; ox < ow; ox++) {                                          \
      const u32a *px = row + (size_t)ox * (N);                                 \
      uint32_t se = 0, so = 0;                                                 \
      for (int dx = 0; dx < (N); dx++) {                                       \
        uint32_t v = px[dx];                                                   \
        se += v & SWAR_LANES;                                                  \
        so += (v >> 8) & SWAR_LANES;                                           \
      }                                                                        \
      e[ox] += se;                                                             \
      o[ox] += so;                                                             \
    }                                                                          \
  }

/* One accumulated channel value out of the lane sums e and o ([byte] 0..3 in memory order). */
static inline int swar_channel(uint32_t e, uint32_t o, int byte) {
  uint32_t w = (byte & 1) ? o : e;
  return (int)((w >> ((byte >> 1) * 16)) & 0xFFFF);
}

/* Widest scaled output row (in pixels) the per-thread accumulators can hold. */
#define MEAN_MAX_OW 4096
/*
 * Fills [acc] with the mean blue, green and red of every output pixel of the [n] source rows starting at [src_y0], using the SWAR sums above.
 */
static inline void scaled_row_means(const unsigned char *src, int stride,
                                    int ow, int n, int src_y0, int ob, int og,
                                    int or_, int *acc) {
  uint32_t e[MEAN_MAX_OW], o[MEAN_MAX_OW];
  memset(e, 0, (size_t)ow * sizeof(uint32_t));
  memset(o, 0, (size_t)ow * sizeof(uint32_t));
  switch (n) {
  case 2:
    SWAR_ROWS(2) break;
  case 3:
    SWAR_ROWS(3) break;
  case 4:
    SWAR_ROWS(4) break;
  default:
    SWAR_ROWS(n) break;
  }
  const int inv = n * n;
  switch (n) {
  case 2:
    for (int ox = 0; ox < ow; ox++) {
      acc[ox * 3] = swar_channel(e[ox], o[ox], ob) / 4;
      acc[ox * 3 + 1] = swar_channel(e[ox], o[ox], og) / 4;
      acc[ox * 3 + 2] = swar_channel(e[ox], o[ox], or_) / 4;
    }
    break;
  case 3:
    for (int ox = 0; ox < ow; ox++) {
      acc[ox * 3] = swar_channel(e[ox], o[ox], ob) / 9;
      acc[ox * 3 + 1] = swar_channel(e[ox], o[ox], og) / 9;
      acc[ox * 3 + 2] = swar_channel(e[ox], o[ox], or_) / 9;
    }
    break;
  case 4:
    for (int ox = 0; ox < ow; ox++) {
      acc[ox * 3] = swar_channel(e[ox], o[ox], ob) / 16;
      acc[ox * 3 + 1] = swar_channel(e[ox], o[ox], og) / 16;
      acc[ox * 3 + 2] = swar_channel(e[ox], o[ox], or_) / 16;
    }
    break;
  default:
    for (int ox = 0; ox < ow; ox++) {
      acc[ox * 3] = swar_channel(e[ox], o[ox], ob) / inv;
      acc[ox * 3 + 1] = swar_channel(e[ox], o[ox], og) / inv;
      acc[ox * 3 + 2] = swar_channel(e[ox], o[ox], or_) / inv;
    }
  }
}

/* Per-thread scratch for two output rows of means: 3 channels x up to 4096
   output pixels (8192 wide at scale 2). */
static __thread int t_mean0[MEAN_MAX_OW * 3];
/* Per-thread scratch rows for scaled_row_means. */
static __thread int t_mean1[MEAN_MAX_OW * 3];

/*
 * Converts one strip of chroma rows [cy0, cy1) with box-averaging downscale: Y and CbCr come from the mean of each scale x scale block. Only rows marked dirty are converted.
 */
static inline void convert_strip_scaled(const conv_job_t *j) {
  const int n = j->scale, stride = j->stride, ow = j->ow;
  const int or_ = g_off_r, og = g_off_g, ob = g_off_b;
  if (ow > MEAN_MAX_OW)
    return; /* mode checks keep it below this */
  for (int cy = j->cy0; cy < j->cy1; cy++) {
    if (!row_is_dirty(j->dirty, cy))
      continue;
    unsigned char *yo0 = j->ydst + (size_t)(cy * 2) * ow;
    unsigned char *yo1 = yo0 + ow;
    unsigned char *uv = j->uvdst + (size_t)cy * ow;
    /* The two output luma rows this chroma row covers. */
    scaled_row_means(j->src, stride, ow, n, (cy * 2) * n, ob, og, or_, t_mean0);
    scaled_row_means(j->src, stride, ow, n, (cy * 2 + 1) * n, ob, og, or_,
                     t_mean1);
    for (int ox = 0; ox < ow; ox += 2) {
      const int *m00 = t_mean0 + ox * 3, *m01 = t_mean0 + (ox + 1) * 3;
      const int *m10 = t_mean1 + ox * 3, *m11 = t_mean1 + (ox + 1) * 3;
      yo0[ox] =
          (unsigned char)(((47 * m00[2] + 157 * m00[1] + 16 * m00[0] + 128) >>
                           8) +
                          16);
      yo0[ox + 1] =
          (unsigned char)(((47 * m01[2] + 157 * m01[1] + 16 * m01[0] + 128) >>
                           8) +
                          16);
      yo1[ox] =
          (unsigned char)(((47 * m10[2] + 157 * m10[1] + 16 * m10[0] + 128) >>
                           8) +
                          16);
      yo1[ox + 1] =
          (unsigned char)(((47 * m11[2] + 157 * m11[1] + 16 * m11[0] + 128) >>
                           8) +
                          16);
      int ab = (m00[0] + m01[0] + m10[0] + m11[0]) >> 2;
      int ag = (m00[1] + m01[1] + m10[1] + m11[1]) >> 2;
      int ar = (m00[2] + m01[2] + m10[2] + m11[2]) >> 2;
      int cb = (((-25 * ar - 87 * ag + 112 * ab + 128) >> 8) + 128);
      int cr = (((112 * ar - 102 * ag - 10 * ab + 128) >> 8) + 128);
      if (cb < 0)
        cb = 0;
      else if (cb > 255)
        cb = 255;
      if (cr < 0)
        cr = 0;
      else if (cr > 255)
        cr = 255;
      uv[ox] = (unsigned char)cb;
      uv[ox + 1] = (unsigned char)cr;
    }
  }
}

/*
 * Converts one strip of chroma rows [cy0, cy1) at native size (scale 1) to NV12 with BT.709 limited-range coefficients. Only rows marked dirty are converted.
 */
static inline void convert_strip(const conv_job_t *j) {
  const int w = j->w, stride = j->stride;
  const int or_ = g_off_r, og = g_off_g, ob = g_off_b;
  for (int cy = j->cy0; cy < j->cy1; cy++) {
    if (!row_is_dirty(j->dirty, cy))
      continue;
    int y0 = cy * 2, y1 = y0 + 1;
    const unsigned char *row0 = j->src + (size_t)y0 * stride;
    const unsigned char *row1 = j->src + (size_t)y1 * stride;
    unsigned char *yo0 = j->ydst + (size_t)y0 * w;
    unsigned char *yo1 = j->ydst + (size_t)y1 * w;
    unsigned char *uv = j->uvdst + (size_t)cy * w;
    for (int x = 0; x < w; x += 2) {
      const unsigned char *p;
      int b, g, r, sb, sg, sr;
      p = row0 + (size_t)x * 4;
      b = p[ob];
      g = p[og];
      r = p[or_];
      yo0[x] = (unsigned char)(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
      sb = b;
      sg = g;
      sr = r;
      p = row0 + (size_t)(x + 1) * 4;
      b = p[ob];
      g = p[og];
      r = p[or_];
      yo0[x + 1] =
          (unsigned char)(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
      sb += b;
      sg += g;
      sr += r;
      p = row1 + (size_t)x * 4;
      b = p[ob];
      g = p[og];
      r = p[or_];
      yo1[x] = (unsigned char)(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
      sb += b;
      sg += g;
      sr += r;
      p = row1 + (size_t)(x + 1) * 4;
      b = p[ob];
      g = p[og];
      r = p[or_];
      yo1[x + 1] =
          (unsigned char)(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
      sb += b;
      sg += g;
      sr += r;
      int ar = sr >> 2, ag = sg >> 2, ab = sb >> 2; /* 2x2 chroma average */
      int cb = (((-25 * ar - 87 * ag + 112 * ab + 128) >> 8) + 128);
      int cr = (((112 * ar - 102 * ag - 10 * ab + 128) >> 8) + 128);
      if (cb < 0)
        cb = 0;
      else if (cb > 255)
        cb = 255;
      if (cr < 0)
        cr = 0;
      else if (cr > 255)
        cr = 255;
      uv[x] = (unsigned char)cb;
      uv[x + 1] = (unsigned char)cr;
    }
  }
}

/* Most conversion threads the pool can hold. */
#define MAX_CONV_THREADS 8
/* Number of threads converting each frame, including the caller. */
static int g_nthreads = 1;
/* Worker thread handles; index 0 is unused because the caller converts strip 0. */
static pthread_t g_pool[MAX_CONV_THREADS];
/* The strip each thread converts for the current frame. */
static conv_job_t g_jobs[MAX_CONV_THREADS];
/* Protects the pool state below. */
static pthread_mutex_t g_pool_mtx = PTHREAD_MUTEX_INITIALIZER;
/* Signalled to wake the workers for a new frame. */
static pthread_cond_t g_pool_go = PTHREAD_COND_INITIALIZER;
/* Signalled when the last worker has finished. */
static pthread_cond_t g_pool_done = PTHREAD_COND_INITIALIZER;
/* Frame generation, bumped to dispatch work. */
static int g_pool_gen = 0;    /* bumped to dispatch a frame */
static int g_pool_active = 0; /* worker jobs still running this gen */
static int g_pool_shutdown = 0;

/*
 * Worker thread: waits for a new generation, converts its strip at nice +2 so it never starves the compositor, and signals completion.
 */
static void *conv_worker(void *arg) {
  int _ign = nice(2); /* Background priority to prevent starving GNOME Shell /
                         Mutter compositor */
  (void)_ign;
  int id = (int)(intptr_t)arg;
  int last_gen = 0;
  for (;;) {
    pthread_mutex_lock(&g_pool_mtx);
    while (g_pool_gen == last_gen && !g_pool_shutdown)
      pthread_cond_wait(&g_pool_go, &g_pool_mtx);
    if (g_pool_shutdown) {
      pthread_mutex_unlock(&g_pool_mtx);
      break;
    }
    last_gen = g_pool_gen;
    pthread_mutex_unlock(&g_pool_mtx);

    if (g_jobs[id].scale > 1)
      convert_strip_scaled(&g_jobs[id]);
    else
      convert_strip(&g_jobs[id]);

    pthread_mutex_lock(&g_pool_mtx);
    if (--g_pool_active == 0)
      pthread_cond_signal(&g_pool_done);
    pthread_mutex_unlock(&g_pool_mtx);
  }
  return NULL;
}

/*
 * Starts the pool: one thread per four online CPUs, between 1 and 4; the caller of bgra_to_nv12 does strip 0 itself.
 */
static void conv_pool_init(void) {
  long n = sysconf(_SC_NPROCESSORS_ONLN);
  g_nthreads = (int)(n / 4);
  if (g_nthreads < 1)
    g_nthreads = 1;
  if (g_nthreads > 4)
    g_nthreads = 4;
  /* Worker threads handle jobs 1..n-1; the caller runs job 0 itself. */
  for (int i = 1; i < g_nthreads; i++)
    pthread_create(&g_pool[i], NULL, conv_worker, (void *)(intptr_t)i);
  fprintf(stderr,
          "[evdi-helper] NV12 conversion using %d thread(s) (nice +2)\n",
          g_nthreads);
}

/*
 * Converts the whole BGRA framebuffer [src] to NV12 in [dst] using the pool, splitting the chroma rows evenly; [dirty] limits the work to stale rows (NULL converts everything). Returns when all strips are done.
 */
static void bgra_to_nv12(const unsigned char *src, unsigned char *dst,
                         const unsigned char *dirty) {
  int ch = g_out_h / 2;
  unsigned char *ydst = dst;
  unsigned char *uvdst = dst + (size_t)g_out_w * g_out_h;
  for (int i = 0; i < g_nthreads; i++) {
    g_jobs[i] = (conv_job_t){src,
                             ydst,
                             uvdst,
                             g_mode_w,
                             g_mode_h,
                             g_mode_stride,
                             g_out_w,
                             g_out_h,
                             g_scale,
                             ch * i / g_nthreads,
                             ch * (i + 1) / g_nthreads,
                             dirty};
  }
  if (g_nthreads > 1) {
    pthread_mutex_lock(&g_pool_mtx);
    g_pool_active = g_nthreads - 1;
    g_pool_gen++;
    pthread_cond_broadcast(&g_pool_go);
    pthread_mutex_unlock(&g_pool_mtx);
  }
  if (g_jobs[0].scale > 1)
    convert_strip_scaled(&g_jobs[0]);
  else
    convert_strip(&g_jobs[0]); /* caller does its own strip */
  if (g_nthreads > 1) {
    pthread_mutex_lock(&g_pool_mtx);
    while (g_pool_active > 0)
      pthread_cond_wait(&g_pool_done, &g_pool_mtx);
    pthread_mutex_unlock(&g_pool_mtx);
  }
}

/*
 * Marks every chroma row stale in all three buffers' masks, so the next conversion of each buffer is complete.
 */
static void mark_all_dirty(void) {
  if (!g_dirty_fill)
    return;
  memset(g_dirty_fill, 0xFF, (size_t)g_dirty_bytes);
  memset(g_dirty_latest, 0xFF, (size_t)g_dirty_bytes);
  memset(g_dirty_write, 0xFF, (size_t)g_dirty_bytes);
}

/* Record damaged chroma rows into every buffer's mask: each of them now lacks
   this content until it is individually reconverted. */
static void mark_damage(const struct evdi_rect *rects, int n) {
  if (!g_dirty_fill || g_chroma_rows <= 0)
    return;
  for (int i = 0; i < n; i++) {
    int y0 = rects[i].y1, y1 = rects[i].y2;
    if (y1 < y0) {
      int t = y0;
      y0 = y1;
      y1 = t;
    }
    /* Source rows map onto output chroma rows through the scale: one
       chroma row covers 2*scale source rows. */
    int div = 2 * g_scale;
    int c0 = y0 / div, c1 = (y1 + div - 1) / div;
    if (c0 < 0)
      c0 = 0;
    if (c1 > g_chroma_rows)
      c1 = g_chroma_rows;
    for (int cy = c0; cy < c1; cy++) {
      unsigned char bit = (unsigned char)(1u << (cy & 7));
      g_dirty_fill[cy >> 3] |= bit;
      g_dirty_latest[cy >> 3] |= bit;
      g_dirty_write[cy >> 3] |= bit;
    }
  }
}
