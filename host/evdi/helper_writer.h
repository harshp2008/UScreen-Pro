/*
 * helper_writer.h: publishing a converted frame and the writer thread that paces it into the capture FIFO.
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It owns the triple-buffer hand-over and the FIFO; it does not decide when a frame is grabbed.
 */
// PLATFORM(linux): writes frames into a named pipe (FIFO)

/* Convert the stride-padded BGRA framebuffer into the NV12 fill buffer and
   publish it as the latest frame. Called from the event-loop thread. */
static void publish_frame(void) {
  if (!g_buffers_ready || !g_framebuffer)
    return;

  long long t_c0 = now_us();
  /* Only the rows this particular buffer is missing. */
  bgra_to_nv12(g_framebuffer, g_fill, g_dirty_fill);
  memset(g_dirty_fill, 0, (size_t)g_dirty_bytes);
  g_conv_us = now_us();
  g_conv_sum_us += g_conv_us - t_c0;
  g_conv_n++;

  pthread_mutex_lock(&g_swap_mutex);
  unsigned char *tmp = g_latest;
  g_latest = g_fill;
  g_fill = tmp;
  /* The masks describe the buffers, so they travel with them. */
  unsigned char *dtmp = g_dirty_latest;
  g_dirty_latest = g_dirty_fill;
  g_dirty_fill = dtmp;
  g_latest_valid = 1;
  g_latest_req_us = g_req_us;
  g_latest_ready_us = g_ready_us;
  g_latest_grab_us = g_grab_us;
  g_latest_conv_us = g_conv_us;
  pthread_cond_signal(&g_frame_ready);
  pthread_mutex_unlock(&g_swap_mutex);
}

/* (Re)open the capture FIFO without blocking forever: O_NONBLOCK open fails
   with ENXIO while no reader (ffmpeg) has the other end open. */
static int try_open_fifo(void) {
  /* O_NOFOLLOW: the path is in a private directory now, but a symlink
     planted there must still never redirect the screen into a file. */
  int fd = open(g_fifo_path, O_WRONLY | O_NONBLOCK | O_NOFOLLOW);
  if (fd < 0)
    return -1;
  /* Switch back to blocking writes once connected */
  int flags = fcntl(fd, F_GETFL);
  fcntl(fd, F_SETFL, flags & ~O_NONBLOCK);
  /* Enlarge the pipe so a full-frame write doesn't take hundreds of
     64KB round-trips with the encoder. A large request can be refused (the
     per-user pipe budget) while a smaller increase is allowed, so step down
     rather than give up. */
  for (int size = 1 << 20; size >= 1 << 16; size >>= 1)
    if (fcntl(fd, F_SETPIPE_SZ, size) >= 0)
      break;
  fprintf(stderr, "[evdi-helper] Capture FIFO opened\n");
  return fd;
}

/* Writer thread: paces frames strictly at 1/fps intervals using high-resolution
   timers (clock_nanosleep with CLOCK_MONOTONIC and TIMER_ABSTIME).
   Zero timer drift, zero frame bursts, and anti-burst drift correction. */
static void *writer_thread(void *arg) {
  (void)arg;
  struct timespec next_frame_time;
  clock_gettime(CLOCK_MONOTONIC, &next_frame_time);
  int have_frame = 0;
  int frame_generation = -1;

  while (g_running) {
    int cur_fps = g_throttled ? 1 : (g_fps > 0 ? g_fps : 60);
    long long period_ns = 1000000000LL / cur_fps;

    if (g_capture_fifo_fd < 0) {
      g_capture_fifo_fd = try_open_fifo();
      if (g_capture_fifo_fd < 0) {
        /* No reader yet — poll slowly instead of spinning. */
        struct timespec idle = {.tv_sec = 0, .tv_nsec = 50000000L};
        nanosleep(&idle, NULL);
        clock_gettime(CLOCK_MONOTONIC, &next_frame_time);
        continue;
      }
      clock_gettime(CLOCK_MONOTONIC, &next_frame_time);
    }

    /* 1. Precision sleep until the scheduled frame release time using
     * TIMER_ABSTIME */
    clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &next_frame_time, NULL);
    if (!g_running)
      break;

    /* 2. Advance schedule for next frame and apply anti-burst drift correction
     */
    struct timespec now_ts;
    clock_gettime(CLOCK_MONOTONIC, &now_ts);
    timespec_add_ns(&next_frame_time, period_ns);
    /* If write took too long or thread was delayed, resync next_frame_time to
     * prevent bursts */
    if (timespec_diff_ns(&next_frame_time, &now_ts) <= 0) {
      next_frame_time = now_ts;
      timespec_add_ns(&next_frame_time, period_ns);
    }

    /* 3. Extract the latest frame under lock */
    pthread_mutex_lock(&g_swap_mutex);
    if (g_full_refresh_req) {
      g_full_refresh_req = 0;
      mark_all_dirty();
    }
    if (!g_buffers_ready) {
      pthread_mutex_unlock(&g_swap_mutex);
      continue;
    }
    if (frame_generation != g_mode_generation) {
      /* Buffers were reallocated; previous g_write content is gone */
      frame_generation = g_mode_generation;
      have_frame = 0;
    }
    int fresh = 0;
    if (g_latest_valid) {
      unsigned char *tmp = g_write;
      g_write = g_latest;
      g_latest = tmp;
      unsigned char *dtmp = g_dirty_write;
      g_dirty_write = g_dirty_latest;
      g_dirty_latest = dtmp;
      g_latest_valid = 0;
      g_write_req_us = g_latest_req_us;
      g_write_ready_us = g_latest_ready_us;
      g_write_grab_us = g_latest_grab_us;
      g_write_conv_us = g_latest_conv_us;
      have_frame = 1;
      fresh = 1;
    }
    int size = g_packed_size;
    g_writer_busy = have_frame;
    pthread_mutex_unlock(&g_swap_mutex);

    if (!have_frame)
      continue; /* nothing grabbed yet for this mode */

    /* In Max Performance Mode (g_idle_keepalive_ms <= 16), every frame tick is
       sent, providing a locked 60 FPS stream to the tablet with zero drops. In
       standard mode, motionless screens are kept alive every
       g_idle_keepalive_ms (33ms). */
    long long now_ms_write = now_ms();
    int flush_due = !fresh && g_flush_pending && g_flush_enabled;
    if (!fresh && !flush_due &&
        (now_ms_write - g_last_write_ms) < g_idle_keepalive_ms) {
      g_writer_busy = 0;
      continue;
    }

    /* Suppress re-sending stale frames containing a departed cursor during
       the boundary transition (< 100ms after exit), until the compositor's
       clean frame arrives. */
    if (!fresh && g_last_exit_time_us > 0) {
      long long since_exit_ms = (now_us() - g_last_exit_time_us) / 1000;
      if (since_exit_ms < 100) {
        if (flush_due)
          g_flush_blocked_n++;
        g_writer_busy = 0;
        continue;
      }
    }
    g_last_write_ms = now_ms_write;
    if (flush_due) {
      g_flush_pending = 0;
      g_flush_sent_n++;
    }
    if (fresh)
      g_fresh_sent_n++;

    /* Bounded write: if the encoder stops reading for >250ms per chunk
       it is stalled or dead — close the FIFO and resync on reopen. */
    long long t_write_start = now_us();
    const unsigned char *ptr = g_write;
    size_t remaining = (size_t)size;
    while (remaining > 0 && g_running) {
      struct pollfd wfd = {.fd = g_capture_fifo_fd, .events = POLLOUT};
      int pr = poll(&wfd, 1, 250);
      if (pr <= 0 || (wfd.revents & (POLLERR | POLLHUP))) {
        fprintf(stderr, "[evdi-helper] Encoder not reading — closing FIFO\n");
        close(g_capture_fifo_fd);
        g_capture_fifo_fd = -1;
        break;
      }
      ssize_t written = write(g_capture_fifo_fd, ptr, remaining);
      if (written <= 0) {
        if (errno == EINTR)
          continue;
        fprintf(stderr, "[evdi-helper] FIFO write failed: %s\n",
                strerror(errno));
        close(g_capture_fifo_fd);
        g_capture_fifo_fd = -1;
        break;
      }
      ptr += written;
      remaining -= (size_t)written;
    }
    g_writer_busy = 0;
    long long t_fifo_done = now_us();
    if (remaining == 0) {
      g_write_sum_us += t_fifo_done - t_write_start;
      g_write_n++;
    }

    uint32_t seq = g_fifo_seq++;
    if (g_timing_shm && remaining == 0) {
      uint32_t idx = seq % TIMING_RING_SIZE;
      g_timing_shm->records[idx].seq = seq;
      g_timing_shm->records[idx].is_fresh = fresh ? 1 : 0;
      g_timing_shm->records[idx].t_request_us = (uint64_t)g_write_req_us;
      g_timing_shm->records[idx].t_ready_us = (uint64_t)g_write_ready_us;
      g_timing_shm->records[idx].t_grab_done_us = (uint64_t)g_write_grab_us;
      g_timing_shm->records[idx].t_conv_done_us = (uint64_t)g_write_conv_us;
      g_timing_shm->records[idx].t_fifo_write_us = (uint64_t)t_fifo_done;
      __sync_synchronize();
    }

    /* Arm the flush duplicate for an isolated fresh frame (see the comment at
       g_flush_enabled), and pull the next writer tick forward so it goes out
       ~2 ms later instead of a whole frame period later. */
    if (fresh && remaining == 0) {
      long long tf = now_us();
      int sparse = g_last_fresh_write_us == 0 ||
                   (tf - g_last_fresh_write_us) >= FLUSH_SPARSE_US;
      g_last_fresh_write_us = tf;
      g_flush_pending =
          (sparse && g_flush_enabled && !g_throttled &&
           g_idle_keepalive_ms > 16);
      if (g_flush_pending) {
        g_flush_armed_n++;
        struct timespec soon;
        clock_gettime(CLOCK_MONOTONIC, &soon);
        timespec_add_ns(&soon, FLUSH_DELAY_NS);
        if (timespec_diff_ns(&soon, &next_frame_time) < 0)
          next_frame_time = soon;
      }
    }

    /* Only freshly grabbed frames say anything about capture latency */
    if (fresh && remaining == 0 && g_write_grab_us > 0 &&
        g_lat_count < LAT_SAMPLES) {
      long long d = now_us() - g_write_grab_us;
      if (d >= 0 && d < 1000000)
        g_lat_us[g_lat_count++] = (int)d;
    }
  }
  return NULL;
}
