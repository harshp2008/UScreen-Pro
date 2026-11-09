/*
 * helper_signals.h: POSIX signal handlers (stop, throttle, resume).
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It sets flags only; acting on them is up to the event loop and the writer thread.
 */

/**
 * POSIX Signal Handlers & EVDI Event Loop State Transitions
 *
 * EVDI Helper participates in an event-driven display pipeline driven by the
 * EVDI kernel module, DRM/KMS compositor notifications, and host daemon
 * supervisor signals:
 *
 * 1. SIGTERM / SIGINT (Termination Signal Handler):
 *    - Triggered by `kill -TERM`, host daemon shutdown, or user interrupt.
 *    - Sets `g_running = 0` atomically.
 *    - Transitions the event loop out of `run_event_loop()`.
 *    - Unblocks `pthread_cond_timedwait(&g_frame_ready, ...)` in
 * `writer_thread()`.
 *    - Signals the dynamic boundary sync thread to terminate.
 *    - Ensures graceful cleanup: closing the EVDI handle, draining FIFO
 * buffers, destroying X11 boundary nudge windows, and unmapping the shared
 * timing ring.
 *
 * 2. SIGUSR1 (Throttle to Standby / Keep-Alive Mode):
 *    - Dispatched by the host daemon when an attached client disconnects or
 * transitions to background sleep, or during prolonged idle desktop states.
 *    - Sets `g_throttled = 1` atomically.
 *    - State Transition: Alters the event loop cadence from nominal (e.g. 60
 * FPS / 16ms) down to 1 FPS keep-alive standby (1000ms period, poll timeout
 * 50ms).
 *    - Rationale: Eliminates unnecessary CPU/GPU memory bandwidth consumption
 * and thermal dissipation when no display client is consuming frames, while
 * still generating periodic keep-alive frames to satisfy downstream encoder and
 * player timeouts.
 *
 * 3. SIGUSR2 (Resume Active Streaming & Keyframe Request):
 *    - Dispatched by the host daemon upon client reconnect or foreground
 * activation.
 *    - Sets `g_throttled = 0` atomically, restoring nominal frame rate pacing.
 *    - State Transition: Invokes `mark_all_dirty()`, setting the damage mask
 * across all triple-buffer slots (`g_dirty_fill`, `g_dirty_latest`,
 * `g_dirty_write`).
 *    - Rationale: Guarantees that the very first frame sent to the returning
 * client is a complete, non-differential keyframe containing full desktop
 * contents rather than incremental damage rects.
 */
static void handle_signal(int sig) {
  (void)sig;
  g_running = 0;
}

/* Set from the SIGUSR2 handler, consumed by the writer thread under
   g_swap_mutex. The handler must not call mark_all_dirty() itself: the writer
   rotates the three dirty-mask pointers under that mutex, so a memset from
   signal context can hit one mask twice and miss another, leaving stale rows
   (artifacts) after a client reconnect. */
static volatile sig_atomic_t g_full_refresh_req = 0;

/*
 * SIGUSR1 throttles capture to the 1 fps standby rate; SIGUSR2 restores the normal rate and asks the writer thread for a full refresh.
 */
static void handle_throttle(int sig) {
  if (sig == SIGUSR1) {
    g_throttled = 1;
  } else if (sig == SIGUSR2) {
    g_throttled = 0;
    g_full_refresh_req = 1;
  }
}
