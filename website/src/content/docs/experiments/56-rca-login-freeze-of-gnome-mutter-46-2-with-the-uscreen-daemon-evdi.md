---
title: "56 RCA: login freeze of GNOME (mutter 46.2) with the UScreen daemon / EVDI"
sidebar:
  order: 40
  label: "56 RCA: login freeze of GNOME (mutter 46.2) with the UScreen"
---

Status: **fix implemented and unit/integration-tested; live verification on real EVDI hardware NOT yet done** (needs `sudo modprobe evdi` and the tablet; see §7). Root cause is **evidence-supported, not proven by a captured trace** (see §3).

| Field | Value |
| :-- | :-- |
| **ID** | 56 |
| **Date** | not recorded |
| **Build** | `e4c1387`, `bb422e2`, `adc91ac` (as written in the file) |
| **Verdict** | not recorded |
| **Raw results** | Kept in the repository under `experiments_and_findings/56_evidence/`; not published. |

## What was tested

Environment: Ubuntu, GNOME Shell on Wayland, mutter 46.2, kernel 7.0.0-42, evdi 1.14.15 (`initial_device_count=2`), i915 primary (card1), EVDI = card0/card2. `MUTTER_DEBUG_DISABLE_HW_CURSORS=1` is set in the session.

### Evidence gathered (raw excerpts in `56_evidence/`)

Boots: `-1` = failed login (20:02–20:13, ended by hard reset), `0` = second failing login (20:14, recovered by the emergency commands at ~20:26), `-2` = healthy 28 h run with the same daemon, `-3` = healthy 46 h run.

| # | Observation | Source |
|---|---|---|
| E1 | The literal Phase 1 filters return **nothing**: `journalctl -b -1 -p err -g "evdi\|mutter\|gnome-shell\|segfault\|drm\|grab\|pointer"` → `-- No entries --`, same for boot 0. No kernel oops/BUG/hung-task/flip_done timeout, no OOM kill (the one "timed out" hit is the Wi-Fi driver `rtw89_8852be … timed out to flush queues`). No coredump tooling installed; no gnome-shell/Xwayland crash line. | `phase1_literal_filters.txt` |
| E2 | Boot -1 ends abruptly at 20:13:25.9 in the middle of ffmpeg progress lines — no shutdown sequence (hard power-off). gnome-shell was still alive and mapping windows 30 s after the daemon started (`Invalid window geometry for xdg_surface@50` 20:12:53). So: **not a crash; a stall/livelock, or a partially working desktop.** | `boot-1_failed_login_timeline.txt` |
| E3 | The daemon starts **2.9 s after gnome-shell** and creates the virtual monitor immediately: `gnome-shell[6671] Running GNOME Shell` 20:12:22.625 → `Started uscreen.service` 20:12:25.482 → `Helper connected on card0` 20:12:25.973 → `Connector state: connected` → `Edid property set` 20:12:26.021. At 20:12:25.39, just before, the shell logged `Adaptive Dock: _updateMonitorsCache failed: Timeout was reached` — its main loop was already congested by extension start-up (15+ extensions). | boot -1 timeline |
| E4 | **Hotplug cascade in the first 2 s**: helper connects (25.97) → "Display mode change … (310x194mm) → (266x166mm)" restarts the helper → `Disconnecting…` (27.049) → reconnect (27.707) → `Edid property set` again (27.928) → mode change notified (28.437). Plus two `gsettings` writes, `busctl GetCurrentState`, uinput devices created. 8.5 s later a **second** tablet (Wi-Fi) adds card2/DVI-I-2, with mutter re-creating a GBM renderer (`g_hash_table_steal_extended … failed`, 9× `MESA-LOADER`, `Created gbm renderer for card2`, rtkit flipping the KMS thread RT↔normal 8× in 50 ms). | boot -1 timeline |
| E5 | The unit is only `After=graphical-session.target`, which GNOME reaches at session-manager start, before the shell finished loading. `grep` over `host/src` finds **no compositor-readiness check** anywhere in the daemon. | `scripts/uscreen.service`, code |
| E6 | `keep_alive = true` in `~/.config/uscreen/config.toml`. With it, `capture.rs` (old line 1189) starts the helper **at daemon start with no tablet attached**: a connected, consumer-less monitor exists from login (boot 0: `Helper connected on card0` 20:14:37, `connectors: eDP-1, DVI-I-1`; tablet attaches 20:15:07). The code's own comment (issue #12) calls this out as harmful. It explains "second screen black, taskbar visible, windows invisible" (docks live on a monitor nobody sees) and "occurs with no tablet connected". **But see E8: boot -2 did the same and stayed healthy, so this is a contributing/hardening factor, not a sufficient cause.** | `keepalive_and_hotadd_counts.txt`, `boot0_tablet_connect_timeline.txt` |
| E7 | Boot 0 trigger: tablet attaches at 20:15:07 → "Tablet is a screen" → "Display mode change" → helper restart → `Can't update stage views actor … needs an allocation` ×5 (20:15:08.79) → `Gio.IOErrorEnum: Timeout was reached` in gnome-shell 8 s later. | `boot0_tablet_connect_timeline.txt` |
| E8 | **Healthy contrast** (boot -2): the daemon started 2.9 s after the shell, **also created a consumer-less monitor at login** (`Helper connected on card0` 12:49:56) and stayed healthy; the tablet attached 12 h later (01:25) with the same 2340x1080→2560x1600 mode change and GPU re-add, no incident. Mutter's GPU hot-add signature (`g_hash_table_steal_extended` + `Created gbm renderer`) also occurs in healthy boots (110 in boot -3, 35 in boot -2, `keepalive_and_hotadd_counts.txt`). **So neither early start, nor a consumer-less monitor, nor GPU hot-add alone is the discriminator** — three hypotheses I rejected on this evidence. A host-code diff between the healthy build (`e4c1387`) and the failing one (`bb422e2`) shows only notices/restart-nonce plumbing and a comment in the helper; no change to the display path. **The only pattern that separates the boots: in both failing boots a tablet attach / mode change / helper restart happened within the first ~35 s of the session (boot -1: +3 s…+20 s, two devices; boot 0: +33 s); in the healthy boots it happened hours in, on a settled desktop.** | `boot-2_healthy_contrast.txt` |
| E9 | The EVDI views have no accelerated sharing: `Failed to initialize accelerated iGPU/dGPU framebuffer sharing: Not hardware accelerated` for card0 and card2 on every boot, so mutter copies each frame for the EVDI outputs through the CPU. Upstream: mutter MR 615 documents that this CPU `glReadPixels` path *blocks mutter for the duration of the copy*. | boot logs; [MR 615](https://gitlab.gnome.org/GNOME/mutter/merge_requests/615) |
| E10 | External corroboration of the class of failure: gnome-shell lock-ups when an EVDI GPU is hot-added ([Launchpad #1873401](https://bugs.launchpad.net/bugs/1873401)), multi-GPU micro-freezes ([mutter #1616](https://gitlab.gnome.org/GNOME/mutter/-/issues/1616)), secondary-GPU outputs freezing ([mutter #2923](https://gitlab.gnome.org/GNOME/mutter/-/issues/2923)). None is an exact match; none shows a fix in 46.2. | web |

### Hypothesis

**H:** The freeze is a *compositor main-loop stall* triggered by the daemon changing the display topology of a CPU-copied EVDI monitor (create, mode-change, re-create, add a second) **during the first minute of the session, while gnome-shell is still busy with start-up** (extensions, docks, input mapping), with no readiness gate and no spacing of hotplugs. A consumer-less monitor (`keep_alive`) widens the window but is not sufficient on its own (E8). Everything on the primary screen that needs a frame/main-loop turn (keyboard shortcuts, drag, which needs continuous repaint) starves, while simple actor clicks can still land; the dummy monitor shows docks but no windows.

Falsifiable predictions and results so far:

* P1 "A shell that is idle and settled does not stall on the same sequence" — consistent with healthy boots -2/-3 (E8); **not yet tested live**. This is the prediction the 60 s settle gate rests on.
* P2 "No crash artefact exists" — **confirmed** (E1, E2).
* P3 "The daemon had no readiness/hotplug protection" — **confirmed** by code (E5).
* P4 "keep_alive creates a consumer-less monitor at login" — **confirmed** by log and code (E6); **refuted as a sufficient cause** by boot -2 (E8).
* P5 "Click-but-no-drag is a mutter pointer-grab deadlock" — **not supported**: no grab, `XGrab*`, `warp` or barrier code exists in the daemon or helper (`grep`), and nothing in the journal points at a grab. I therefore treat it as a symptom of starvation (a drag needs sustained frame production), not as a separate bug. I found no input-device deadlock evidence.


## Result

### Changes

All in the repo; nothing under `~/.config`, `/etc` or the user's installed unit was modified.

1. **`host/src/guard.rs` (new)**
   * *Readiness gate* `wait_compositor_ready`: GNOME: shell owns `org.gnome.Shell`, shell process ≥ 60 s old, `IsSessionRunning` true, and `DisplayConfig.GetCurrentState` (served from the shell's **main loop**; a `Peer.Ping` would be answered by a GDBus worker thread and not detect a stall) answers ≤ 500 ms four times in a row. Other desktops: Wayland socket present + 5 s settle. It waits, logs every 15 s, ends on shutdown, and only falls back after 180 s on a shell that answers but stays busy.
   * *Hotplug limiter*: ≥ 3 s between helper start/stop, max 4 changes per 60 s, then 30 s cool-down.
   * *Stall watchdog* `spawn_watchdog`: while the helper exists, probe every 1 s; a probe over 2 s or timing out (3 s) counts; **3 in a row trip the breaker** → helper and ffmpeg are stopped (monitor removed), retry after 60 s, then 300 s, third trip latches the display off for the session. Healthy for 60 s marks the run healthy. Probe latency is logged per minute, so a future incident leaves evidence in the journal.
   * *Boot-loop guard*: `~/.local/state/uscreen/guard.json` (fsynced, atomic rename) records `display_up`/`healthy`/`tripped`/`stopped` + `boot_id`. If the previous boot ended with the display up and never healthy (what a freeze + hard reset looks like) or a trip occurred, the next start is **safe mode**: no virtual monitor until `uscreen reset-guard` + daemon restart. A normal shutdown records `stopped`, so it does not false-trigger.
2. **`host/src/capture.rs`**: admission through the guard before every `start_helper`; `display_up/down/tripped` bookkeeping at each helper start/stop; watchdog arms in the session `select!`; **`keep_alive` is honoured only after a tablet has actually been attached in this daemon run** (`tablet_seen`), restoring the intent of the #12 comment (hardening; not claimed as the cause).
3. **`host/src/main.rs`**: `mod guard`, `uscreen reset-guard` subcommand.
4. **`packaging/uscreen.service`, `scripts/uscreen.service`**: `After=… gnome-session-initialized.target`, `StartLimitBurst=4/120 s`, `Nice=5`, `CPUWeight=50`, `OOMScoreAdjust=300` (daemon yields to the compositor and is OOM-killed first).
5. **`scripts/rca56_guarded_test.sh`**: supervised live test (§7) with an out-of-process dead-man.

Not changed (deliberately): the EVDI module still loads at boot via `/etc/modules-load.d/uscreen.conf` (needs root; GPU hot-add is normal in healthy boots, E8); the helper's X11 "boundary sync" thread (no evidence against it; candidate follow-up: it polls `XQueryPointer` every 16 ms and may place its 1×1 window at a guessed geometry if XRandR has not listed the output yet).

### Tests run (all on this machine, none touching EVDI or the live compositor)

* `cargo test`: **59 passed**, 0 failed (11 new in `guard`): boot-loop verdict table, JSON round-trip, hotplug spacing/cap, breaker needs 3 consecutive stalls and trips once, backoff 60 s → 300 s → latch.
* `gate_waits_while_the_shell_is_absent`: with no compositor on the bus the gate is **still waiting after 3 s**; `gate_wait_ends_on_shutdown`: a shutdown ends the wait.
* `watchdog_trips_on_a_frozen_compositor_and_not_on_a_healthy_one`: private `dbus-daemon` with a fake `DisplayConfig`; 5 ms answers for 6 s → no trip; answers 60 s late → **tripped 11.0 s after the freeze**.
* Live read-only probe against this session (`guard::tests::live_probes_against_this_session`, `--ignored`): shell uptime parsed 963 s vs `ps` 965 s; `IsSessionRunning = true`; `GetCurrentState` round trips **1.8–5.7 ms** (threshold 2000 ms, margin ≈ 350×). (First version used a non-existent `SessionRunning` *property*; caught by this test, it is a method on GNOME 46.)
* (The readiness threshold was raised from 20 s to 60 s *after* the live probe and smoke test below, which ran with 20 s; the unit tests do not depend on it. The smoke test's "ready after 1.5 s" is therefore with the old threshold, as the shell was already 15 minutes old.)
* Smoke test of the real daemon in a transient user unit (`systemd-run --user`, `RuntimeMaxSec=45`, EVDI unloaded, state dir in a sandbox, systemd autostart untouched), USB tablet attached and `keep_alive=true`: daemon logged `No tablet is a screen — disconnecting the virtual display` at start (**no monitor at login**), then `guard: compositor ready after 1.5s`, then, when the tablet attached, tried the helper and failed cleanly on the missing module with the existing 2/4/8 s backoff. No processes left behind; guard state `stopped`.

### Evidence that the daemon now runs safely (partial)

**Live guarded test on real EVDI, run by the user at 21:09 (`56_evidence/live_20261107_210923/`): `NO STALL; worst probe 450 ms over 424 samples`.**

* Probe of the shell main loop (`GetCurrentState`, every 250 ms, 106 s): median 14 ms, p95 18 ms, p99 44 ms, max 450 ms (threshold 2000 ms). The daemon's own watchdog logged a worst round trip of 208 ms for its first minute.
* Daemon log: `guard: compositor ready after 1.5s` → `Helper connected on card0` → `Compositor negotiated 2560x1600@90Hz` → after 60 s `guard: virtual display has been stable for 60s - marking this run healthy`. No `ERROR`, no breaker trip.
* `gnome-shell.log` for the window: 0 lines matching `Timeout was reached|Invalid sequence|assertion`; `kernel.log`: 0 matches for `BUG:|Oops|timed out|flip_done`. The user reported no freeze and normal desktop interaction.

**What this does and does not show.** It shows that, on this machine, with EVDI loaded and the new daemon, a 2560x1600@90 virtual monitor stays up under a responsive compositor and the safeguards do not misfire. It does **not** show that *login* is safe: the shell was already hours old, so the 1.5 s gate time reflects an old shell, not the 60 s settle path, and no tablet attach/mode change happened in the first minute of a session (the pattern of both freezes, E8). That check is the next login with the service enabled (§7 step 4); until then the root cause stays "evidence-supported, not proven" (§3). Systemd autostart was disabled throughout.

### Remaining verification (needs the user) — final order

1. Rebuild: `cd host && cargo build --release`, and the helper with `make build-helper` (copies it to `target/release/` and `~/.local/bin/`, which is where the service finds it).
2. Install the systemd drop-in `packaging/10-compositor-safety.conf` into `~/.config/systemd/user/uscreen.service.d/`, `daemon-reload`.
3. Restore the software-cursor variable: `sudo cp /etc/environment.bak-rca56 /etc/environment`. **Reboot** (the systemd user manager lingers, so only a reboot re-imports `/etc/environment`, §8.1 fact 5).
4. After the reboot, with the service still disabled: `scripts/rca56_guarded_test.sh 90` (cursor is composited by default now) and check on the tablet: cursor visible, lag, ghost on fast exit; and `grep 'Boundary sync' daemon.log` shows `output DVI-I-1 at (1920,0) …` and no `(1920,165)`.
5. `systemctl --user enable uscreen.service`, log out/in with the tablet attached; expect `guard: compositor ready` before `Helper connected`.
6. After a bad boot the daemon starts in safe mode; `uscreen reset-guard` + restart re-enables it.

### Follow-up: cursor invisible on the virtual screens (found in the guarded test)

Symptom (user): the pointer is visible on the laptop screen but invisible on the EVDI screens; clicks work.

Facts established:

1. `MUTTER_DEBUG_DISABLE_HW_CURSORS=1` is **already** set, globally, in `/etc/environment` (since experiment 03) and in the running session; the journal says `Disabling hardware cursors because MUTTER_DEBUG_DISABLE_HW_CURSORS is set` on every boot. So "force software cursors" is not an available fix: it is the current state. It is a mutter variable, read at shell start; the daemon cannot change it for the running shell.
2. `evdi_helper.c` calls `evdi_enable_cursor_events(handle, true)`: the cursor is delivered as events and left **out** of the grabbed frames, and the helper only logs those events (nothing in the stream draws a cursor). Experiment 46 chose this deliberately to stop the burned-in/ghost cursor.
3. With mutter's hardware cursor disabled there is no cursor plane update, so EVDI delivers **0** `ON_CURSOR_*` events (measured: 0 in the daemon log with events on).
4. Live A/B on the real tablet (`adb screencap` of the tablet while the pointer is on the virtual monitor): helper switch `USCREEN_CURSOR_EVENTS=1` (current) and `=0` (libevdi composites the cursor into frames) — **no cursor visible in either**, using three input methods (absolute uinput pointer with a click that demonstrably landed on the tablet's search box; relative uinput mouse). The stream does show everything else, so the stream path works. Screenshots in the session scratchpad only; not archived.

Interpretation (not proven): with software cursors, mutter is not painting the cursor into the framebuffer that is copied to the EVDI output, and with hardware cursors disabled the EVDI cursor plane is never used, so neither EVDI mode has a cursor to deliver. Exp 03 observed a burned-in cursor on an earlier setup; I could not reproduce that today.

Change made: `evdi_helper.c` honours `USCREEN_CURSOR_EVENTS=0|1` (default unchanged: on) and logs the choice at start, so the two modes can be compared without rebuilding. `make build-helper` also refreshed `target/release/evdi_helper` and `~/.local/bin/evdi_helper` (default behaviour identical).

**Not yet tested, requires a re-login** (the only combination that can differ): hardware cursors enabled in mutter (remove the variable from `/etc/environment`) together with `USCREEN_CURSOR_EVENTS=0`, so libevdi composites the EVDI cursor plane into the stream. Risk: this brings back the original ghost-cursor setup that experiment 05's boundary-sync thread was written for; revert by restoring the variable.

### 1 Update after the user's `USCREEN_CURSOR_EVENTS=0` run

Result reported by the user: `NO STALL (worst probe 334 ms)`; with `USCREEN_CURSOR_EVENTS=0` the cursor was visible but laggy and left a ghost on the previous screen when moved fast.

New facts found while preparing the next step:

5. **That run was still in software-cursor mode.** The user removed the variable from `/etc/environment` and logged out/in, but the new gnome-shell (pid 52772, started 21:23:16) still logged `Disabling hardware cursors because MUTTER_DEBUG_DISABLE_HW_CURSORS is set` and has it in `/proc/<pid>/environ`. The source is the systemd **user manager** (`systemd --user`, pid 2358, started 20:14:23; `loginctl`: `Linger=yes`), which keeps the environment it imported at its start (from `/etc/environment` via pam) across logouts; gnome-shell is a user service and inherits it. `systemctl --user unset-environment MUTTER_DEBUG_DISABLE_HW_CURSORS` returned success but the variable stayed (a test variable set/unset the same way did work; cause unexplained, I could not read the manager's environ). A reboot restarts the manager and is the reliable fix.
6. So the lag + ghost is the **software-cursor + libevdi-composite** combination, i.e. exactly the mechanism of experiments 03/05 (cursor painted into an idle output's framebuffer, cleared late). It says nothing yet about *hardware cursor* + either mode, which has still not been measured. (Also note: my earlier synthetic-uinput A/B saw no cursor at all, while the user's real mouse does see one in this mode — synthetic devices are not a valid cursor test here; use the real mouse.)

### 2 A/B results from the user, git archaeology, and the boundary-sync defect

User's controlled A/B after the reboot (hardware cursors confirmed: 0 `MUTTER_DEBUG` vars in gnome-shell's environ):

| Mode | Stall probe | Cursor on the tablet |
|---|---|---|
| A: hardware cursors, default (cursor events on) | NO STALL, worst 284 ms | **invisible** |
| B: hardware cursors, `USCREEN_CURSOR_EVENTS=0` | NO STALL, worst 139 ms (reported as "software cursors", but see below) | visible, **severe input lag, ghost cursor stuck when moving fast** |

(B was reported by the user as the software-cursor mode; the shell environment check says hardware cursors were on, so B is hardware plane + libevdi compositing. Either way: stability is unaffected by the cursor mode.)

Reading: EVDI does not deliver the hardware cursor plane usefully — with events on nothing draws it (A); with libevdi compositing it is visible but slow/ghosting (B). The project's earlier answer (experiments 03 and 05, commit `adc91ac`) was to force mutter's software cursor (`MUTTER_DEBUG_DISABLE_HW_CURSORS=1` in `/etc/environment`) and suppress the ghost with the 1x1 X11 nudge window in `evdi_helper.c`.

Archaeology (no code was lost): the whole boundary-sync engine from `adc91ac` — `boundary_sync_thread`, the 1x1 override-redirect `g_nudge_win` resized on exit/entry and on approach within 40 px (25 Hz cap), `g_last_exit_time_us` and the <100 ms stale-frame suppression in `writer_thread`, `-lX11 -lXrandr` in the Makefile — is present in `HEAD` and the working tree. `git log -S g_nudge_win` → only `adc91ac`. History of the cursor flag: `false` (11d6b65, composite) → `true` (adc91ac) → `true` (5821a6d reformat only). Exp 03 specified `false`; so with the env var set and the default `true` the cursor was invisible (the original complaint in the first guarded test).

**Defect found in the existing engine.** `init_boundary_sync()` runs right after the helper connects, before mutter has created the monitor, so XRandR did not list the output and the thread kept a hard-coded rectangle from an old layout: the live log said `Boundary sync initialized: tablet output at (1920,165) 1464x915` while mutter had DVI-I-1 at `(1920,0)` (`GetCurrentState`: `DVI-I-1 x 1920 y 0 scale 1.7486`). Exits/entries were detected against a rectangle 165 px too low, the nudge window sat at y+1=166, and with two tablets every helper matched "any DVI- output" (the same one). Healthy boots only got correct numbers (`(1920,0) 1704x1065`) when a later helper restart happened after the monitor existed.

**Fix (this change, `host/evdi/evdi_helper.c`):**
* geometry is no longer read once: `refresh_tablet_geometry()` re-reads XRandR every 250 ms until the output exists, then every 1 s, logs each change, moves/maps the nudge window onto the output, and treats "output gone" as "pointer not in tablet";
* the output is this helper's own connector, found from `/sys/class/drm/card<N>-<connector>` (card2 → DVI-I-2), instead of any `DVI-*`;
* the hard-coded fallback rectangle is removed; the nudge window is created unmapped until the output is known;
* the default cursor mode is now **composited** (cursor events off), because that is what a visible cursor needs; `USCREEN_CURSOR_EVENTS=1` selects the old exp-46 mode.

Status: compiles clean (`gcc -Wall -Wextra`, no warnings). **Not live-tested** — per the user's instruction no further A/B runs were made; verification is §7 step 4. Whether the corrected geometry removes the lag/ghost is unproven; if the lag persists the next suspects are the CPU cost of composited frames at 2560x1600@90 and the 33 ms idle re-emission in `writer_thread`.

### 3 Final result of the corrected boundary sync (user, after rebuild + reboot with the software-cursor variable restored)

| Check | Result |
|---|---|
| `MUTTER_DEBUG_DISABLE_HW_CURSORS` in gnome-shell's environ | `1` (software cursors active, as intended) |
| Guarded stall tests (2 runs) | **NO STALL** — worst probes 367 ms and 185 ms |
| Helper log | `Boundary sync: output DVI-I-1 at (1920,0) 1464x915` (the real geometry; the stale `(1920,165)` is gone) |
| Helper log | `cursor events off: cursor composited into the stream (default)` |
| User observation | **cursor smooth again: no lag, no ghosting; the system did not freeze** |

So the cursor regression was the boundary-sync geometry defect of §8.2, not an inherent cost of compositing: with the correct per-output rectangle the original `adc91ac` design (software cursor + composited cursor + 1x1 nudge window + stale-frame suppression) works again.

### Resolution and trade-off

* **Stability (the freeze):** the fix is the guard (readiness gate with 60 s shell settle, hotplug limiter, stall watchdog, boot-loop safe mode) plus the unit hardening (`After=gnome-session-initialized.target`, `Nice=5`, `CPUWeight=50`, `OOMScoreAdjust=300`, start limit). Two live guarded runs on real EVDI: no stall (worst probe 450 ms, then 334 ms). Login-time behaviour is verified only by the first real login with the service enabled (§7 step 5); root cause remains evidence-supported, not proven (§3).
* **Status:** cursor confirmed (§8.3); six guarded runs on real EVDI across the session, none stalled (worst probe 450 ms). **Still open:** the first real *login* with the service enabled — the failing scenario of both freezes — has not happened yet; §3 stands (root cause evidence-supported, not proven). The safeguards (settle gate, watchdog, safe mode) are what protect that first login.
* **Cursor:** the project's own design is kept and repaired rather than replaced: software cursors (`/etc/environment`), composited cursor (new default), boundary-sync nudge with live geometry (§8.2). Trade-off: stability is independent of the cursor mode (four guarded runs, none stalled); the tablet cursor is smooth with the corrected boundary sync (§8.3, confirmed live by the user). Hardware cursors + default mode (A) are rejected: invisible cursor.

### Latency baselines (2560x1600 @ 90, Intel iGPU, VAAPI, USB) and what drives them

Measured with `uscreen doctor --latency` (real daemon, tablet attached, synthetic mouse sweep) and `scripts/enc_latency_probe.py` (the daemon's exact ffmpeg arguments fed through a FIFO; latency = frame written → its slice NAL out; synthetic runs include the FIFO write and run beside the idle daemon encoder, so compare rows with each other, not with the real-daemon rows).

**Real daemon, `doctor --latency` (p50 / p95, ms)**

| Run | encoder → tablet screen | capture-ready → screen | host stages |
|---|---|---|---|
| 2560x1600 stream, session 1 | 24–26 / 34 | 120 / 132 | 99 / 108 |
| `stream_scale=2` (1280x800 stream) | 22 / 29 | 64 / 84 | 45 / 67 |
| user's later run, 2560x1600 | 24.0 | 70.4 | 50.0 |

Reading: the USB link plus tablet decode is a steady ~22–26 ms. Everything variable is host-side, and it varies with how densely frames arrive (below), which is why two runs on the same setup gave 120 ms and 70 ms.

**Synthetic encoder probe, 2560x1600 (p50 ms)**

| Question | Result |
|---|---|
| `hevc_vaapi` vs `h264_vaapi` | **HEVC is slower here**: 90 Hz continuous feed, interleaved x3: h264 33–45 vs hevc 54–58; with the frame hold removed (below): h264 29–31 vs hevc 44–76. No upside to HEVC at this bitrate on this GPU unless the link is the limit (it is not: USB takes ~24 ms). |
| Extra CPU colour conversion (swscale) in ffmpeg? | **None.** The filter graph is `rawvideo → format=nv12,hwupload → h264_vaapi`; debug log has 0 `auto_scale`/`sws` lines (libswscale is linked but unused). The only CPU conversion is in `evdi_helper` (BGRA→NV12, 4 threads): 0.7 ms p50 / 1.0 ms p95. Moving it to the GPU (feed BGRA 4 B/px, `hwupload,scale_vaapi=format=nv12`) is **slower**: 33–34 vs 28–29 ms p50 and a worse p95, because the FIFO carries 2.7x the bytes. Keep the CPU conversion. |
| 2560x1600 @ 60 vs 90 | **No real difference** once the hold is removed: 29.4–30.5 vs 29.6–29.9. Per-frame encode cost does not depend on the frame rate; lowering to 60 Hz does not help latency (it only changes the frame interval, below). |
| Resolution | The one thing that costs real encode time: 1920x1080 and 1280x800 ≈ 22 ms vs ≈ 33–60 ms at 2560x1600. |
| VAAPI knobs | `-low_power 1` unsupported on this GPU; `-compression_level 7`, `-quality 7`, `-fps_mode passthrough`, `-flush_packets 1` change nothing material. |

**The dominant effect: ffmpeg + VAAPI hold each frame until the next one arrives.** Latency of a frame ≈ a fixed ~20 ms + roughly its distance to the *next* frame written to the FIFO:

| Feed interval | 11 ms (active motion) | 33 ms (helper idle keepalive) | 100 ms (sparse damage) |
|---|---|---|---|
| p50 latency | 24 ms | 51–52 ms | 119 ms |
| same, with a duplicate of each frame written 2 ms later | – | 27.5 ms | **31 ms** (h264), 46 ms (hevc) |

`-fps_mode passthrough` and `-flush_packets 1` do not remove the hold; a duplicate frame does. This explains the real-daemon `encode` stage of ~75–115 ms at 2560x1600 (the helper only re-emits an unchanged frame every `IDLE_KEEPALIVE_MS` = 33 ms, or 16 ms in Max Performance Mode) and why the cursor feels laggy when updates are sparse but fine when moving fast.

**Implemented: flush duplicate (`evdi_helper.c`, writer thread).** After a fresh frame that follows a quiet period (previous fresh frame >= 25 ms earlier) the helper writes one duplicate of it ~2 ms later by pulling the next writer tick forward. It does nothing during bursts (so continuous motion does not double the encoder load), while throttled, in Max Performance Mode (a frame is sent every 16 ms anyway), and it respects the boundary-exit ghost guard (a flush is skipped, not sent, during the 100 ms window after the pointer leaves the tablet; 0 were blocked in the tests). `USCREEN_FLUSH_DUP=0` disables it; the helper's 5 s stats line now also reports `writes: fresh / flush armed / sent / blocked-by-ghost-guard`.

Real-daemon A/B (`uscreen doctor --latency [--sparse]`, standard pacing i.e. `max_performance_mode` temporarily off, two rounds each, p50 ms):

| | flush OFF | flush ON |
|---|---|---|
| sparse updates (one step / 100 ms): host stages | 114.4, 113.5 | **89.7, 88.3** |
| sparse: capture-ready → screen | 127.2, 126.0 | **102.1, 101.3** |
| continuous motion: host stages | 52.8, 50.3 | 48.4, 49.5 (unchanged, as designed) |
| continuous: capture-ready → screen | 66.9, 63.2 | 61.7, 62.1 |
| flush writes (sparse run) | 0 | ~125-134 sent of ~139-141 armed |

So about **-25 ms (-22%) for isolated updates**, nothing lost elsewhere. This is less than the probe's -90 ms because the duplicate is a whole 6 MB frame that itself takes ~9 ms to write through the FIFO and the real encoder shares the GPU with the compositor. In Max Performance Mode (power profile `performance`) the flush is off by design and sparse host stages measured ~84 ms (p95 ~89) with a 16 ms frame cadence — i.e. the frame cadence alone does not explain the whole host stage; the remaining ~80 ms for a 4 Mpx frame is encode + queueing and is only reduced by a smaller stream (stream detail "Half": ~45 ms).

What users can expect today: ~24 ms link + ~20 ms encode + up to one frame interval of hold (≈ 11 ms while moving, 33 ms at the idle keepalive, up to ~100 ms for isolated updates) at 2560x1600; stream detail "Half" halves the encode share; HEVC and 60 Hz are not latency wins on this hardware.


## Not verified

The journal contains no direct signature of the freeze moment (no stack, no timeout error naming the cause), and I could not reproduce it: loading EVDI needs `sudo` and the trigger needs the tablet, and deliberately freezing the live desktop was not acceptable. The chain E3→E4→E9 plus the early-session timing pattern (E8) is the best-supported explanation and matches the symptoms and the upstream reports, but the precise mutter-internal blocking point is **unconfirmed**. The fix is therefore designed to be correct *whatever* the exact point is: it removes every trigger the logs show and adds a detector that removes the monitor if the compositor stalls anyway.

## Sources

- [`experiments_and_findings/56_evdi_mutter_freeze_rca.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/56_evdi_mutter_freeze_rca.md)
