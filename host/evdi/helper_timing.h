/*
 * helper_timing.h: clocks and the shared-memory ring of per-frame stage timestamps.
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It owns now_ms/now_us and the timing ring; the numbers are produced elsewhere.
 */

static long long now_ms(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

/* Microseconds on CLOCK_MONOTONIC. */
static long long now_us(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (long long)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;
}

/*
 * Creates and maps the shared timing ring file at [path] (mode 0600) and stamps its magic and version. Does nothing for a NULL path; on failure the helper continues without timing.
 */
static void init_timing_shm(const char *path) {
  if (!path)
    return;
  g_timing_shm_fd = open(path, O_RDWR | O_CREAT, 0600);
  if (g_timing_shm_fd < 0) {
    fprintf(stderr, "[evdi-helper] Could not open timing shm %s: %s\n", path,
            strerror(errno));
    return;
  }
  if (ftruncate(g_timing_shm_fd, sizeof(struct SharedTimingRing)) != 0) {
    fprintf(stderr, "[evdi-helper] ftruncate failed on %s: %s\n", path,
            strerror(errno));
    close(g_timing_shm_fd);
    g_timing_shm_fd = -1;
    return;
  }
  void *ptr = mmap(NULL, sizeof(struct SharedTimingRing),
                   PROT_READ | PROT_WRITE, MAP_SHARED, g_timing_shm_fd, 0);
  if (ptr == MAP_FAILED) {
    fprintf(stderr, "[evdi-helper] mmap failed on %s: %s\n", path,
            strerror(errno));
    close(g_timing_shm_fd);
    g_timing_shm_fd = -1;
    return;
  }
  g_timing_shm = (struct SharedTimingRing *)ptr;
  g_timing_shm->magic = TIMING_MAGIC;
  g_timing_shm->version = TIMING_VERSION;
  fprintf(stderr,
          "[evdi-helper] High-resolution timing SHM initialized at %s\n", path);
}

/* Unmaps and closes the timing ring. */
static void cleanup_timing_shm(void) {
  if (g_timing_shm && g_timing_shm != MAP_FAILED) {
    munmap(g_timing_shm, sizeof(struct SharedTimingRing));
    g_timing_shm = NULL;
  }
  if (g_timing_shm_fd >= 0) {
    close(g_timing_shm_fd);
    g_timing_shm_fd = -1;
  }
}
