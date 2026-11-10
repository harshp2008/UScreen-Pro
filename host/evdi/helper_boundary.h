/*
 * helper_boundary.h: dynamic boundary synchronisation: reports when the pointer crosses onto or off the tablet output and nudges the compositor with a tiny X11 window.
 *
 * Part of evdi_helper.c: this file is #included by it exactly once, in the order listed there, so the
 * whole helper is still one translation unit (static functions stay static and the hot loops still inline).
 * It is not a header and cannot be compiled on its own. It owns the X11 connection and the boundary thread; it never touches frames.
 */

/* --- Dynamic Boundary Synchronization (Zero Ghost Cursor & Zero Boundary Lag)
 * --- */
static pthread_t g_boundary_thread = 0;
/* Set while the boundary thread should keep running. */
static volatile int g_boundary_running = 0;
/* Connection to the X server, or NULL when there is none (Wayland-only session). */
static Display *g_x11_dpy = NULL;
/* The 1x1 override-redirect window whose resizing makes the compositor schedule a frame. */
static Window g_nudge_win = 0;
/* Where this helper's output sits on the desktop, in the X root's (logical)
   coordinates. Unknown (w == 0) until the compositor has created the monitor:
   the helper connects BEFORE mutter lists the new output, so reading XRandR
   once at start-up found nothing and the thread ran forever on a hard-coded
   rectangle from an old layout (y off by 165 px), which made it nudge at the
   wrong moments and the ghost cursor survive (RCA 56 s8.2). It is therefore
   re-read while the helper runs. */
static int g_tablet_x = 0;
/* Y of the tablet output's top edge in X root coordinates. */
static int g_tablet_y = 0;
/* Width of the tablet output in pixels. */
static int g_tablet_w = 0;
/* Height of the tablet output in pixels. */
static int g_tablet_h = 0;
/* Whether the output geometry has been found yet. */
static int g_tablet_known = 0;
/* Whether the nudge window has been mapped onto the output. */
static int g_nudge_mapped = 0;
/* Connector name of this helper's card, for example DVI-I-2. */
static char g_my_connector[32] = "";
/* Whether the pointer was on the tablet output at the last sample. */
static volatile int g_pointer_in_tablet = 0;
/*
 * Microsecond time (CLOCK_MONOTONIC) at which the pointer last left the tablet; 0 when it has not left lately.
 */
static volatile long long g_last_exit_time_us = 0;
/* Last POINTER_IN_TABLET value written to stdout; -1 = nothing reported yet. */
static int g_reported_in_tablet = -1;

/* Adds [ns] nanoseconds to [ts], keeping tv_nsec normalised. */
static inline void timespec_add_ns(struct timespec *ts, long long ns) {
  ts->tv_nsec += ns;
  while (ts->tv_nsec >= 1000000000L) {
    ts->tv_nsec -= 1000000000L;
    ts->tv_sec += 1;
  }
}

/* [a] minus [b] in nanoseconds (negative if [a] is earlier). */
static inline long long timespec_diff_ns(const struct timespec *a,
                                         const struct timespec *b) {
  return (long long)(a->tv_sec - b->tv_sec) * 1000000000LL +
         (a->tv_nsec - b->tv_nsec);
}

/* The connector this helper's EVDI card drives ("DVI-I-2" for card2), from
   /sys/class/drm/card<N>-<connector>. With two tablets each helper must watch
   its OWN output; matching any "DVI-" output made both use the same one. */
static void find_my_connector(void) {
  if (g_device_index < 0)
    return;
  char prefix[32];
  snprintf(prefix, sizeof(prefix), "card%d-", g_device_index);
  size_t plen = strlen(prefix);
  DIR *d = opendir("/sys/class/drm");
  if (!d)
    return;
  struct dirent *de;
  while ((de = readdir(d)) != NULL) {
    if (strncmp(de->d_name, prefix, plen) == 0 && de->d_name[plen]) {
      snprintf(g_my_connector, sizeof(g_my_connector), "%s", de->d_name + plen);
      break;
    }
  }
  closedir(d);
}

/* Re-read this output's geometry from XRandR; keeps the nudge window on it. */
static void refresh_tablet_geometry(void) {
  Window root = DefaultRootWindow(g_x11_dpy);
  XRRScreenResources *res = XRRGetScreenResourcesCurrent(g_x11_dpy, root);
  int found = 0, x = 0, y = 0, w = 0, h = 0;
  if (res) {
    for (int i = 0; i < res->noutput && !found; i++) {
      XRROutputInfo *info = XRRGetOutputInfo(g_x11_dpy, res, res->outputs[i]);
      if (info && info->connection == RR_Connected && info->crtc) {
        int mine = g_my_connector[0]
                       ? strcmp(info->name, g_my_connector) == 0
                       : (strstr(info->name, "DVI-") || strstr(info->name, "UScreen") ||
                          strstr(info->name, "evdi"));
        if (mine) {
          XRRCrtcInfo *crtc = XRRGetCrtcInfo(g_x11_dpy, res, info->crtc);
          if (crtc) {
            x = crtc->x;
            y = crtc->y;
            w = (int)crtc->width;
            h = (int)crtc->height;
            found = (w > 0 && h > 0);
            XRRFreeCrtcInfo(crtc);
          }
        }
      }
      if (info)
        XRRFreeOutputInfo(info);
    }
    XRRFreeScreenResources(res);
  }
  if (!found) {
    if (g_tablet_known)
      fprintf(stderr, "[evdi-helper] Boundary sync: output %s went away\n",
              g_my_connector);
    g_tablet_known = 0;
    return;
  }
  if (!g_tablet_known || x != g_tablet_x || y != g_tablet_y || w != g_tablet_w ||
      h != g_tablet_h) {
    g_tablet_x = x;
    g_tablet_y = y;
    g_tablet_w = w;
    g_tablet_h = h;
    fprintf(stderr,
            "[evdi-helper] Boundary sync: output %s at (%d,%d) %dx%d\n",
            g_my_connector[0] ? g_my_connector : "?", x, y, w, h);
    if (g_nudge_win) {
      XMoveWindow(g_x11_dpy, g_nudge_win, x + 1, y + 1);
      if (!g_nudge_mapped) {
        XMapWindow(g_x11_dpy, g_nudge_win);
        g_nudge_mapped = 1;
      }
      XFlush(g_x11_dpy);
    }
  }
  g_tablet_known = 1;
}

/*
 * Boundary thread: every 16 ms reads the pointer, reports crossings as POINTER_IN_TABLET lines, and resizes the nudge window by one pixel when the pointer enters, leaves or approaches (within 40 px) the tablet so the compositor produces a frame at once.
 */
static void *boundary_sync_thread(void *arg) {
  (void)arg;
  struct timespec ts = {
      .tv_sec = 0,
      .tv_nsec = 16000000L}; /* 16ms (~60Hz polling matching frame budget) */
  int toggle = 1;
  long long last_prewarm = 0;

  long long last_geo_us = 0;
  while (g_running && g_boundary_running) {
    nanosleep(&ts, NULL);
    if (!g_x11_dpy || !g_nudge_win)
      continue;

    /* Poll fast until the output exists, then slowly to follow layout /
       scale changes. */
    long long t_us = now_us();
    if (t_us - last_geo_us > (g_tablet_known ? 1000000 : 250000)) {
      last_geo_us = t_us;
      refresh_tablet_geometry();
    }
    if (!g_tablet_known) {
      g_pointer_in_tablet = 0;
      continue;
    }

    Window root_ret, child_ret;
    int rx = 0, ry = 0, wx = 0, wy = 0;
    unsigned int mask = 0;
    if (!XQueryPointer(g_x11_dpy, DefaultRootWindow(g_x11_dpy), &root_ret,
                       &child_ret, &rx, &ry, &wx, &wy, &mask)) {
      continue;
    }

    int in_tablet = (rx >= g_tablet_x && rx < (g_tablet_x + g_tablet_w) &&
                     ry >= g_tablet_y && ry < (g_tablet_y + g_tablet_h));

    /* Tell the daemon each time the pointer crosses onto or off the tablet
       (and once when the output becomes known). It starts the idle-dim timer
       when the pointer leaves. */
    if (in_tablet != g_reported_in_tablet) {
      g_reported_in_tablet = in_tablet;
      printf("POINTER_IN_TABLET %d\n", in_tablet);
      fflush(stdout);
    }

    if (g_pointer_in_tablet && !in_tablet) {
      /* Mouse exited tablet -> laptop screen: force Mutter to schedule
       * immediate clear frame */
      g_pointer_in_tablet = 0;
      g_last_exit_time_us = now_us();
      toggle = (toggle == 1) ? 2 : 1;
      XResizeWindow(g_x11_dpy, g_nudge_win, toggle, 1);
      XFlush(g_x11_dpy);
    } else if (!g_pointer_in_tablet && in_tablet) {
      /* Mouse entered laptop -> tablet screen: wake frame clock */
      g_pointer_in_tablet = 1;
      toggle = (toggle == 1) ? 2 : 1;
      XResizeWindow(g_x11_dpy, g_nudge_win, toggle, 1);
      XFlush(g_x11_dpy);
    } else if (!g_pointer_in_tablet && rx >= (g_tablet_x - 40) &&
               rx < g_tablet_x && ry >= g_tablet_y &&
               ry < (g_tablet_y + g_tablet_h)) {
      /* Mouse approaching tablet boundary (within 40px): pre-warm frame clock
       */
      long long now = now_us();
      if (now - last_prewarm > 40000) { /* 25Hz max during approach */
        last_prewarm = now;
        toggle = (toggle == 1) ? 2 : 1;
        XResizeWindow(g_x11_dpy, g_nudge_win, toggle, 1);
        XFlush(g_x11_dpy);
      }
    }
  }
  return NULL;
}

/*
 * Opens the X display (DISPLAY or :0), finds this card's connector, creates the nudge window and starts the boundary thread. Silent no-op if X is not available.
 */
static void init_boundary_sync(void) {
  const char *disp_env = getenv("DISPLAY");
  if (!disp_env) {
    disp_env = ":0";
  }
  g_x11_dpy = XOpenDisplay(disp_env);
  if (!g_x11_dpy) {
    fprintf(stderr,
            "[evdi-helper] Boundary sync: could not open X11 display %s\n",
            disp_env);
    return;
  }

  Window root = DefaultRootWindow(g_x11_dpy);
  find_my_connector();
  fprintf(stderr,
          "[evdi-helper] Boundary sync initialized for output %s (geometry is "
          "tracked live; unknown until the compositor creates the monitor)\n",
          g_my_connector[0] ? g_my_connector : "?");

  XSetWindowAttributes attr;
  memset(&attr, 0, sizeof(attr));
  attr.override_redirect = True;
  attr.background_pixel = 0;
  /* Created unmapped; refresh_tablet_geometry() moves it onto the output and
     maps it as soon as the output is known. */
  g_nudge_win = XCreateWindow(
      g_x11_dpy, root, 0, 0, 1, 1, 0, CopyFromParent, InputOutput,
      CopyFromParent, CWOverrideRedirect | CWBackPixel, &attr);
  XSelectInput(g_x11_dpy, g_nudge_win, ExposureMask);
  XFlush(g_x11_dpy);
  refresh_tablet_geometry();

  g_boundary_running = 1;
  if (pthread_create(&g_boundary_thread, NULL, boundary_sync_thread, NULL) !=
      0) {
    fprintf(stderr, "[evdi-helper] Failed to start boundary sync thread\n");
    g_boundary_running = 0;
  }
}

/* Stops the boundary thread and closes the X connection. */
static void cleanup_boundary_sync(void) {
  if (g_boundary_running) {
    g_boundary_running = 0;
    if (g_boundary_thread)
      pthread_join(g_boundary_thread, NULL);
    g_boundary_thread = 0;
  }
  if (g_x11_dpy) {
    if (g_nudge_win) {
      XDestroyWindow(g_x11_dpy, g_nudge_win);
      g_nudge_win = 0;
    }
    XCloseDisplay(g_x11_dpy);
    g_x11_dpy = NULL;
  }
}
