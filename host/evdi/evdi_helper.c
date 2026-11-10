/* Enable GNU extensions (clock_nanosleep, madvise, nice, posix_memalign) before any header. */
#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
/* Only the public client API. The headers are upstream libevdi 1.15's, kept
   in sync with the library: the previous copies predated the
   ddcci_data_handler member of evdi_event_context, so a struct one pointer
   too short was being handed to a library that reads that member. */
#include "evdi_lib.h"
#include <X11/Xlib.h>
#include <X11/extensions/Xrandr.h>

#include "helper_state.h"
#include "helper_signals.h"
#include "helper_boundary.h"
#include "helper_timing.h"
#include "helper_convert.h"
#include "helper_writer.h"
#include "helper_events.h"

/*
 * Index N of the first EVDI card (/dev/dri/cardN) found under /sys/devices/platform/evdi.*, or -1 if there is none.
 */
static int find_evdi_device(void) {
  DIR *dir = opendir("/sys/devices/platform");
  if (!dir)
    return -1;

  struct dirent *entry;
  int found = -1;
  while ((entry = readdir(dir)) != NULL) {
    if (strncmp(entry->d_name, "evdi.", 5) != 0)
      continue;

    char drm_path[512];
    snprintf(drm_path, sizeof(drm_path), "/sys/devices/platform/%s/drm",
             entry->d_name);

    DIR *drm_dir = opendir(drm_path);
    if (!drm_dir)
      continue;

    struct dirent *drm_entry;
    while ((drm_entry = readdir(drm_dir)) != NULL) {
      if (strncmp(drm_entry->d_name, "card", 4) != 0)
        continue;
      int card = atoi(drm_entry->d_name + 4);
      if (card >= 0) {
        found = card;
      }
    }
    closedir(drm_dir);
    if (found >= 0)
      break;
  }
  closedir(dir);
  return found;
}

/* Polls for an EVDI card every 100 ms for up to [timeout_ms]; returns its index or -1. */
static int wait_for_device(int timeout_ms) {
  int waited = 0;
  const int step = 100;
  while (waited < timeout_ms) {
    int idx = find_evdi_device();
    if (idx >= 0)
      return idx;
    usleep(step * 1000);
    waited += step;
  }
  return -1;
}

/*
 * Entry point.
 * Options: --edid FILE (required), --capture-fifo PATH, --scale 1-4, --card N, --fps N (1-240), --max-performance, --timing-shm PATH.
 * Environment: USCREEN_CURSOR_EVENTS=1 leaves the cursor out of the stream, USCREEN_FLUSH_DUP=0 turns the flush duplicate off, USCREEN_BOUNDARY_NUDGE=0 disables boundary sync.
 * Opens (or creates) the EVDI card, connects it with the EDID, then with a FIFO starts the conversion pool and the writer thread, starts boundary sync and runs the capture loop until a signal stops it. Prints EVDI_CONNECTED, MODE_CHANGED and POINTER_IN_TABLET lines to stdout for the daemon; returns 1 on any setup failure.
 */
int main(int argc, char *argv[]) {
  const char *edid_path = NULL;
  const char *fifo_path = NULL;

  for (int i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--edid") == 0 && i + 1 < argc) {
      edid_path = argv[++i];
    } else if (strcmp(argv[i], "--capture-fifo") == 0 && i + 1 < argc) {
      fifo_path = argv[++i];
    } else if (strcmp(argv[i], "--scale") == 0 && i + 1 < argc) {
      g_scale = atoi(argv[++i]);
      if (g_scale < 1)
        g_scale = 1;
      if (g_scale > 4)
        g_scale = 4;
    } else if (strcmp(argv[i], "--card") == 0 && i + 1 < argc) {
      /* Pin a specific EVDI card. With several virtual displays each
         helper must own its own; find_evdi_device() would hand every
         one of them the same card. */
      g_pin_card = atoi(argv[++i]);
    } else if (strcmp(argv[i], "--fps") == 0 && i + 1 < argc) {
      g_fps = atoi(argv[++i]);
      if (g_fps < 1 || g_fps > 240)
        g_fps = 60;
    } else if (strcmp(argv[i], "--max-performance") == 0) {
      g_idle_keepalive_ms = 16;
      fprintf(stderr, "[evdi-helper] Max Performance Mode enabled: idle "
                      "keepalive set to 16ms (~60 FPS)\n");
    } else if (strcmp(argv[i], "--timing-shm") == 0 && i + 1 < argc) {
      g_timing_shm_path = argv[++i];
    }
  }

  if (!edid_path) {
    fprintf(stderr,
            "Usage: %s --edid <edid.bin> [--capture-fifo <path>] [--fps <n>] "
            "[--scale <1-4>]\n",
            argv[0]);
    return 1;
  }

  {
    pthread_condattr_t ca;
    pthread_condattr_init(&ca);
    pthread_condattr_setclock(&ca, CLOCK_MONOTONIC);
    pthread_cond_init(&g_frame_ready, &ca);
    pthread_condattr_destroy(&ca);
  }

  struct sigaction sa;
  memset(&sa, 0, sizeof(sa));
  sa.sa_handler = handle_signal;
  sigaction(SIGINT, &sa, NULL);
  sigaction(SIGTERM, &sa, NULL);
  signal(SIGPIPE, SIG_IGN);

  struct sigaction sa_th;
  memset(&sa_th, 0, sizeof(sa_th));
  sa_th.sa_handler = handle_throttle;
  sa_th.sa_flags = SA_RESTART;
  sigaction(SIGUSR1, &sa_th, NULL);
  sigaction(SIGUSR2, &sa_th, NULL);

  /* Reuse an existing EVDI device if one is free (e.g. from a previous
     run) — adding a new DRM card on every restart floods the compositor
     with display hotplug events. */
  evdi_handle handle = EVDI_INVALID_HANDLE;
  int dev_idx = g_pin_card >= 0 ? g_pin_card : find_evdi_device();
  if (dev_idx >= 0) {
    handle = evdi_open(dev_idx);
    if (handle != EVDI_INVALID_HANDLE) {
      fprintf(stderr, "[evdi-helper] Reusing EVDI device /dev/dri/card%d\n",
              dev_idx);
    }
  }

  if (handle == EVDI_INVALID_HANDLE) {
    fprintf(stderr, "[evdi-helper] Creating EVDI device...\n");
    int written = evdi_add_device();
    if (written < 0) {
      fprintf(stderr, "[evdi-helper] Failed to add EVDI device (err=%d)\n",
              written);
      return 1;
    }

    fprintf(stderr, "[evdi-helper] Waiting for EVDI device...\n");
    dev_idx = wait_for_device(5000);
    if (dev_idx < 0) {
      fprintf(stderr,
              "[evdi-helper] EVDI device did not appear within timeout.\n"
              "[evdi-helper] Either the evdi kernel module is not loaded, or "
              "no device exists\n"
              "[evdi-helper] and /sys/devices/evdi/add is root-only. Check "
              "`lsmod | grep evdi`;\n"
              "[evdi-helper] then, once: echo 'options evdi "
              "initial_device_count=2' | sudo tee "
              "/etc/modprobe.d/uscreen-evdi.conf\n"
              "[evdi-helper]            sudo modprobe -r evdi; sudo modprobe "
              "evdi   (or reboot)\n"
              "[evdi-helper] The packages and install.sh do this — unless "
              "evdi-dkms failed to build, see docs/installation.md.\n");
      return 1;
    }
    fprintf(stderr, "[evdi-helper] Found EVDI device at /dev/dri/card%d\n",
            dev_idx);

    handle = evdi_open(dev_idx);
    if (handle == EVDI_INVALID_HANDLE) {
      fprintf(stderr,
              "[evdi-helper] Failed to open EVDI device /dev/dri/card%d\n",
              dev_idx);
      return 1;
    }
  }
  g_device_index = dev_idx;
  g_handle = handle;
  /* true = the driver delivers the cursor as cursor_set/cursor_move events and
     leaves it OUT of the grabbed framebuffer. false makes libevdi composite
     the cursor into every grabbed frame (burned in, with ghosts when the
     compositor stops repainting an idle output). Measured on the tablet in
     experiments_and_findings/46: false => ~11 grabs/s and a cursor sprite in
     the stream while the pointer moves; true => 0 grabs and a clean stream. */
  {
    /* Default: libevdi composites the cursor into the grabbed frames (cursor
       events OFF). Under hardware cursors EVDI never receives the cursor
       plane and, with events ON, nothing in the stream draws one, so the
       tablet shows NO cursor; under mutter's forced software cursors the
       composited cursor is visible and the boundary-sync engine above keeps
       it from ghosting. USCREEN_CURSOR_EVENTS=1 selects the experiment-46
       mode (cursor left out of the stream). Measured: RCA 56 s8. */
    const char *ce = getenv("USCREEN_CURSOR_EVENTS");
    int cursor_events = (ce && ce[0] == '1');
    evdi_enable_cursor_events(handle, cursor_events);
    fprintf(stderr, "[evdi-helper] cursor events %s (%s)\n",
            cursor_events ? "on: cursor left out of the stream"
                          : "off: cursor composited into the stream",
            ce ? "USCREEN_CURSOR_EVENTS" : "default");
  }

  {
    const char *fd_env = getenv("USCREEN_FLUSH_DUP");
    g_flush_enabled = !(fd_env && fd_env[0] == '0');
    fprintf(stderr, "[evdi-helper] flush duplicate %s\n",
            g_flush_enabled ? "on" : "off (USCREEN_FLUSH_DUP=0)");
  }

  FILE *f = fopen(edid_path, "rb");
  if (!f) {
    fprintf(stderr, "[evdi-helper] Failed to open EDID file: %s\n", edid_path);
    evdi_close(handle);
    return 1;
  }
  fseek(f, 0, SEEK_END);
  long edid_size = ftell(f);
  if (edid_size <= 0 || edid_size > 32768) {
    fprintf(stderr, "[evdi-helper] Invalid EDID size: %ld\n", edid_size);
    fclose(f);
    evdi_close(handle);
    return 1;
  }
  fseek(f, 0, SEEK_SET);
  unsigned char *edid = malloc((size_t)edid_size);
  if (!edid) {
    fprintf(stderr, "[evdi-helper] Failed to allocate EDID buffer\n");
    fclose(f);
    evdi_close(handle);
    return 1;
  }
  size_t read_bytes = fread(edid, 1, (size_t)edid_size, f);
  fclose(f);
  if ((long)read_bytes != edid_size) {
    fprintf(stderr, "[evdi-helper] EDID read error: got %zu of %ld bytes\n",
            read_bytes, edid_size);
    free(edid);
    evdi_close(handle);
    return 1;
  }

  fprintf(stderr, "[evdi-helper] Connecting with EDID (%ld bytes)...\n",
          edid_size);
  evdi_connect(handle, edid, (unsigned int)edid_size, 0);
  free(edid);

  printf("EVDI_CONNECTED card%d\n", dev_idx);
  fflush(stdout);

  pthread_t writer = 0;
  if (fifo_path) {
    g_fifo_path = fifo_path;
    if (g_timing_shm_path) {
      init_timing_shm(g_timing_shm_path);
    }
    conv_pool_init(); /* spawn NV12 conversion workers before first grab */
    fprintf(stderr, "[evdi-helper] Capture FIFO: %s (opened on demand)\n",
            fifo_path);
    if (pthread_create(&writer, NULL, writer_thread, NULL) != 0) {
      fprintf(stderr, "[evdi-helper] Failed to start writer thread\n");
      return 1;
    }
  }

  fprintf(stderr,
          "[evdi-helper] Connected. Capture at %d fps. Entering event loop.\n",
          g_fps);
  {
    const char *nudge = getenv("USCREEN_BOUNDARY_NUDGE");
    if (nudge && nudge[0] == '0')
      fprintf(stderr, "[evdi-helper] Boundary nudge disabled (software cursor mode)\n");
    else
      init_boundary_sync();
  }
  run_event_loop(handle);
  cleanup_boundary_sync();

  g_running = 0;
  /* Wake the writer out of its condition wait so shutdown is immediate
     rather than up to one frame period late. */
  pthread_mutex_lock(&g_swap_mutex);
  pthread_cond_broadcast(&g_frame_ready);
  pthread_mutex_unlock(&g_swap_mutex);
  if (writer)
    pthread_join(writer, NULL);
  if (g_capture_fifo_fd >= 0)
    close(g_capture_fifo_fd);
  free(g_framebuffer);
  free(g_fill);
  free(g_latest);
  free(g_write);
  free(g_dirty_fill);
  free(g_dirty_latest);
  free(g_dirty_write);

  fprintf(stderr, "[evdi-helper] Disconnecting...\n");
  evdi_disconnect(handle);
  evdi_close(handle);
  g_handle = EVDI_INVALID_HANDLE;
  cleanup_timing_shm();

  fprintf(stderr, "[evdi-helper] Done.\n");
  return 0;
}
