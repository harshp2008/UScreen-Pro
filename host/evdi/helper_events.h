/*
 * helper_events.h: the EVDI callbacks (mode, update ready, DPMS, cursor) and the event loop that drives the capture cycle.
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It owns buffer (re)allocation on a mode change and the request/grab pacing.
 */
// PLATFORM(evdi): callbacks and event loop of libevdi (mode change, update ready, DPMS, cursor)

static void on_dpms(int dpms_mode, void *user_data) {
  (void)user_data;
  g_dpms_on = (dpms_mode == 0) ? 1 : 0;
  fprintf(stderr, "[evdi-helper] DPMS: %d (%s)\n", dpms_mode,
          g_dpms_on ? "ON" : "OFF");
}

/* Whether buffer 0 is registered with the EVDI kernel driver. */
static int g_buffer_registered = 0;

/*
 * EVDI mode event. Logs and reports the size on stdout (MODE_CHANGED), sets the pixel order, and, unless the geometry is unchanged, waits (at most 1 s) for the writer, unregisters and frees the old buffers, allocates a huge-page backed framebuffer and the NV12 buffers and masks, registers the framebuffer with the driver and publishes a first dark-grey frame.
 */
static void on_mode_changed(struct evdi_mode mode, void *user_data) {
  (void)user_data;
  fprintf(stderr, "[evdi-helper] Mode: %dx%d@%dHz %dbpp fmt=0x%x\n", mode.width,
          mode.height, mode.refresh_rate, mode.bits_per_pixel,
          mode.pixel_format);
  printf("MODE_CHANGED %d %d %d\n", mode.width, mode.height, mode.refresh_rate);
  fflush(stdout);

  /* Before the "unchanged" early return below: a compositor can switch
     format without changing the size. Conversions are not running during a
     mode event, so the next frame simply uses the new order. */
  set_pixel_order(mode.pixel_format);
  mark_all_dirty();

  int new_w = mode.width;
  int new_h = mode.height;
  int new_bpp = mode.bits_per_pixel / 8;
  if (new_bpp < 1)
    new_bpp = 4;

  /* Same geometry (KWin re-applying the mode)? Keep everything as-is.
     Re-registering on every event is what crashed libevdi before. */
  if (g_buffer_registered && new_w == g_mode_w && new_h == g_mode_h &&
      new_bpp == g_mode_bpp) {
    fprintf(stderr, "[evdi-helper] Mode unchanged, keeping buffer\n");
    g_update_pending = 0;
    return;
  }

  g_mode_w = new_w;
  g_mode_h = new_h;
  g_mode_bpp = new_bpp;

  int row_bytes = g_mode_w * g_mode_bpp;
  int aligned_stride =
      (row_bytes + 63) & ~63; /* DRM buffers are 64-byte aligned */
  g_mode_stride = aligned_stride;
  g_fb_size = g_mode_stride * g_mode_h;

  pthread_mutex_lock(&g_swap_mutex);
  g_latest_valid = 0;
  g_buffers_ready = 0;
  g_mode_generation++;
  pthread_mutex_unlock(&g_swap_mutex);

  /* Wait (bounded) for the writer to finish any in-flight write before
     freeing the buffer it's reading. Writer stalls are bounded because
     FIFO writes poll with a timeout, but never spin here forever — a
     stuck event loop blocks KWin's output handling. */
  for (int i = 0; i < 1000 && g_writer_busy; i++)
    usleep(1000);
  if (g_writer_busy) {
    fprintf(stderr, "[evdi-helper] Writer stuck during mode change — leaking "
                    "old buffers\n");
    /* Deliberately leak instead of freeing under the writer: a one-off
       leak beats a use-after-free. */
    g_fill = NULL;
    g_latest = NULL;
    g_write = NULL;
  }

  /* The kernel must drop its reference to the old framebuffer BEFORE we
     free it — freeing first is a use-after-free during a pending grab. */
  if (g_buffer_registered) {
    evdi_unregister_buffer(g_handle, 0);
    g_buffer_registered = 0;
  }

  free(g_framebuffer);
  /* The kernel copies the damaged part of the scanout buffer into this
     on every grab — the whole 22 MB when the compositor reports full
     damage, which KWin does for this output. That copy is a large share
     of the capture cycle (measured 4-7 ms at 2960x1848), and copy_to_user
     into ordinary 4 KiB pages pays a TLB miss every page. Ask for
     transparent huge pages and touch the memory once now, so the copies
     run over 2 MiB mappings that are already faulted in. */
  {
    size_t huge = 2u << 20;
    size_t len = ((size_t)g_fb_size + huge - 1) / huge * huge;
    void *p = NULL;
    if (posix_memalign(&p, huge, len) == 0 && p) {
      madvise(p, len, MADV_HUGEPAGE);
      memset(p, 0, len);
      g_framebuffer = p;
    } else {
      g_framebuffer = malloc(g_fb_size);
    }
  }

  /* Stream dimensions: source divided by the scale, forced even because
     NV12 chroma covers 2x2 luma samples. */
  g_out_w = (g_mode_w / g_scale) & ~1;
  g_out_h = (g_mode_h / g_scale) & ~1;
  if (g_out_w < 2)
    g_out_w = 2;
  if (g_out_h < 2)
    g_out_h = 2;

  /* Packed buffers hold NV12 (Y plane + half-size interleaved CbCr). */
  g_packed_size = g_out_w * g_out_h * 3 / 2;
  free(g_fill);
  g_fill = malloc(g_packed_size);
  free(g_latest);
  g_latest = malloc(g_packed_size);
  free(g_write);
  g_write = malloc(g_packed_size);

  /* Fresh buffers hold nothing, so every row is stale in all of them. */
  g_chroma_rows = g_out_h / 2;
  g_dirty_bytes = (g_chroma_rows + 7) / 8;
  free(g_dirty_fill);
  g_dirty_fill = malloc((size_t)g_dirty_bytes);
  free(g_dirty_latest);
  g_dirty_latest = malloc((size_t)g_dirty_bytes);
  free(g_dirty_write);
  g_dirty_write = malloc((size_t)g_dirty_bytes);
  if (g_dirty_fill && g_dirty_latest && g_dirty_write)
    mark_all_dirty();

  if (!g_framebuffer || !g_fill || !g_latest || !g_write || !g_dirty_fill ||
      !g_dirty_latest || !g_dirty_write) {
    fprintf(stderr, "[evdi-helper] Failed to allocate framebuffers\n");
    g_have_mode = 0;
    return;
  }

  /* Dark gray initial frame so the tablet shows something immediately */
  memset(g_framebuffer, 0x18, g_fb_size);

  struct evdi_buffer buf = {
      .id = 0,
      .buffer = g_framebuffer,
      .width = g_mode_w,
      .height = g_mode_h,
      .stride = g_mode_stride,
      .rects = NULL,
      .rect_count = 0,
  };
  evdi_register_buffer(g_handle, buf);
  g_buffer_registered = 1;

  pthread_mutex_lock(&g_swap_mutex);
  g_buffers_ready = 1;
  pthread_cond_signal(&g_frame_ready);
  pthread_mutex_unlock(&g_swap_mutex);
  publish_frame();

  g_have_mode = 1;
  g_update_pending = 0;
  fprintf(stderr,
          "[evdi-helper] Buffer 0 registered: %dx%d stride=%d (row_bytes=%d)\n",
          g_mode_w, g_mode_h, buf.stride, row_bytes);
}

/*
 * Asks the driver for the damaged rectangles of the pending update (up to 64; more than that means repaint everything), records the damage in every buffer's mask and publishes the converted frame.
 */
static void grab_now(void) {
  struct evdi_rect rects[64];
  int num_rects = 64;
  long long t0 = now_us();
  evdi_grab_pixels(g_handle, rects, &num_rects);
  g_grab_sum_us += now_us() - t0;
  g_grab_n++;
  if (num_rects <= 0)
    g_empty_n++;
  if (num_rects > 0) {
    g_grab_count++;
    g_grab_us = now_us();
    g_last_exit_time_us = 0;
    /* Under the swap lock: the writer thread reassigns the mask pointers
       when it takes a frame, so touching them unlocked would race.
       If the driver returned more rectangles than we gave it room for,
       we cannot know what else changed — repaint everything. */
    pthread_mutex_lock(&g_swap_mutex);
    if (num_rects >= (int)(sizeof(rects) / sizeof(rects[0])))
      mark_all_dirty();
    else
      mark_damage(rects, num_rects);
    pthread_mutex_unlock(&g_swap_mutex);
    publish_frame();
  }
}

/*
 * EVDI update-ready event: the compositor has committed a frame; records the wait statistics and grabs it.
 */
static void on_update_ready(int buffer_to_be_updated, void *user_data) {
  (void)user_data;
  (void)buffer_to_be_updated;
  g_ready_us = now_us();
  long long wait_us = 0;
  if (g_update_pending) {
    wait_us = g_ready_us - g_req_us;
    g_wait_sum_us += wait_us;
    g_wait_n++;
  }
  g_update_pending = 0;
  grab_now();
}

/* EVDI CRTC state event: logged only. */
static void on_crtc_state(int state, void *user_data) {
  (void)user_data;
  fprintf(stderr, "[evdi-helper] CRTC state: %d\n", state);
}

/* EVDI cursor-set event: logged only (the pointer is drawn by the compositor, not captured here). */
static void on_cursor_set(struct evdi_cursor_set cursor_set, void *user_data) {
  (void)user_data;
  fprintf(stderr,
          "[evdi-helper] ON_CURSOR_SET enabled=%d hot=(%d,%d) size=(%ux%u)\n",
          cursor_set.enabled, cursor_set.hot_x, cursor_set.hot_y,
          cursor_set.width, cursor_set.height);
}

/* EVDI cursor-move event: logged only. */
static void on_cursor_move(struct evdi_cursor_move cursor_move,
                           void *user_data) {
  (void)user_data;
  fprintf(stderr, "[evdi-helper] ON_CURSOR_MOVE pos=(%d,%d)\n", cursor_move.x,
          cursor_move.y);
}

/*
 * The capture loop. Polls the EVDI event fd (4 ms normally, 50 ms throttled, 100 ms before the first mode), dispatches events, requests a new frame at the target rate when none is pending (never requests or grabs while one is pending, which corrupts the driver's state), and logs statistics every 5 s.
 */
static void run_event_loop(evdi_handle handle) {
  struct evdi_event_context evtctx = {
      .dpms_handler = on_dpms,
      .mode_changed_handler = on_mode_changed,
      .update_ready_handler = on_update_ready,
      .crtc_state_handler = on_crtc_state,
      .cursor_set_handler = on_cursor_set,
      .cursor_move_handler = on_cursor_move,
      .user_data = NULL,
  };

  struct pollfd fds[1];
  fds[0].fd = evdi_get_event_ready(handle);
  fds[0].events = POLLIN;

  long long last_stats_ms = now_ms();
  long long stats_grab_base = 0;
  struct timespec next_request_time;
  clock_gettime(CLOCK_MONOTONIC, &next_request_time);

  while (g_running) {
    int cur_fps = g_throttled ? 1 : (g_fps > 0 ? g_fps : 60);
    long long request_period_ns = 1000000000LL / cur_fps;

    struct timespec now_ts;
    clock_gettime(CLOCK_MONOTONIC, &now_ts);
    long long due_diff_ns = timespec_diff_ns(&next_request_time, &now_ts);
    int timeout_ms = 4;
    if (g_throttled) {
      timeout_ms = 50;
    } else if (!g_have_mode) {
      timeout_ms = 100;
    } else if (g_update_pending) {
      /* Waiting for update_ready from compositor. */
      timeout_ms = 16;
    } else {
      timeout_ms = (int)(due_diff_ns / 1000000LL);
      if (timeout_ms < 0)
        timeout_ms = 0;
      if (timeout_ms > 4)
        timeout_ms = 4; /* stay responsive to events */
    }

    int ret = poll(fds, 1, timeout_ms);
    if (ret < 0) {
      if (errno == EINTR)
        continue;
      fprintf(stderr, "[evdi-helper] poll() error: %s\n", strerror(errno));
      break;
    }

    if (ret > 0 && (fds[0].revents & POLLIN)) {
      /* update_ready / mode_changed handlers fire from here */
      evdi_handle_events(handle, &evtctx);
    }

    if (!g_have_mode)
      continue;

    clock_gettime(CLOCK_MONOTONIC, &now_ts);
    due_diff_ns = timespec_diff_ns(&next_request_time, &now_ts);

    /* Core capture cycle: request a fresh frame from the compositor at
       the target fps strictly paced to avoid flooding Mutter or EVDI. */
    if (!g_update_pending && due_diff_ns <= 0) {
      timespec_add_ns(&next_request_time, request_period_ns);
      clock_gettime(CLOCK_MONOTONIC, &now_ts);
      if (timespec_diff_ns(&next_request_time, &now_ts) <= 0) {
        next_request_time = now_ts;
        timespec_add_ns(&next_request_time, request_period_ns);
      }
      g_req_us = now_us();
      if (evdi_request_update(handle, 0)) {
        g_immediate_n++;
        g_ready_us = g_req_us;
        grab_now();
      } else {
        g_update_pending = 1;
      }
    }

    /* Note: When g_update_pending is true, do NOT call grab_now() or
       evdi_request_update(). Calling evdi_grab_pixels without update_ready
       or requesting while already requested corrupts the EVDI driver state,
       flooding the kernel log and causing Mutter compositor mutex stalls.
       The kernel driver will fire update_ready as soon as the compositor
       commits. */

    long long now = now_ms();
    if (now - last_stats_ms >= 5000) {
      double elapsed = (now - last_stats_ms) / 1000.0;
      long long grabs = g_grab_count - stats_grab_base;
      fprintf(stderr,
              "[evdi-helper] %.1f grabs/s (total %lld), mode:%d dpms:%d "
              "pending:%d\n",
              elapsed > 0 ? grabs / elapsed : 0, g_grab_count, g_have_mode,
              g_dpms_on, g_update_pending);
      fprintf(stderr,
              "[evdi-helper] writes: fresh %lld, flush armed %lld sent %lld blocked-by-ghost-guard %lld\n",
              g_fresh_sent_n, g_flush_armed_n, g_flush_sent_n, g_flush_blocked_n);
      fprintf(stderr,
              "[evdi-helper] stages: req→ready avg %.1fms (%d waited, %d imm), "
              "grab avg %.1fms, conv avg %.1fms, write avg %.1fms\n",
              g_wait_n ? g_wait_sum_us / 1000.0 / g_wait_n : 0.0, g_wait_n,
              g_immediate_n, g_grab_n ? g_grab_sum_us / 1000.0 / g_grab_n : 0.0,
              g_conv_n ? g_conv_sum_us / 1000.0 / g_conv_n : 0.0,
              g_write_n ? g_write_sum_us / 1000.0 / g_write_n : 0.0);
      g_wait_sum_us = 0;
      g_wait_n = 0;
      g_grab_sum_us = 0;
      g_grab_n = 0;
      g_immediate_n = 0;
      g_empty_n = 0;
      g_conv_sum_us = 0;
      g_conv_n = 0;
      g_write_sum_us = 0;
      g_write_n = 0;

      /* Capture-side latency: grab → convert → into the encoder's FIFO. */
      pthread_mutex_lock(&g_swap_mutex);
      int n = g_lat_count;
      int snapshot[LAT_SAMPLES];
      if (n > 0)
        memcpy(snapshot, g_lat_us, (size_t)n * sizeof(int));
      g_lat_count = 0;
      pthread_mutex_unlock(&g_swap_mutex);
      if (n > 0) {
        qsort(snapshot, (size_t)n, sizeof(int), cmp_int);
        fprintf(
            stderr,
            "[evdi-helper] capture→fifo p50 %.1fms p95 %.1fms (%d frames)\n",
            snapshot[n / 2] / 1000.0, snapshot[(int)((n - 1) * 0.95)] / 1000.0,
            n);
      }

      stats_grab_base = g_grab_count;
      last_stats_ms = now;
    }
  }
}
