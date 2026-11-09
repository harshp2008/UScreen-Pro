---
title: "Experiments"
sidebar:
  order: 0
  label: "Index"
---

One record for each write-up in `experiments_and_findings/`. The write-ups are working notes kept while the code was changed, not lab reports: some record a measurement, many record a bug, a fix and how it was checked. The pages keep the sentences of the original and move them into one layout.

Each record opens with a short paragraph and a table (ID, date, build, verdict, raw results), then has the sections What was tested, Result, Not verified and Sources. A field the write-up does not state is written "not recorded". Nothing was re-run to write these pages.

**Dates.** Most write-ups carry no date, so the groups below are ranges of IDs, not dates. Where a date is in the write-up it is in the record.

**Build.** The build is the commit hash, with `-dirty` when the tree had uncommitted changes, exactly as the write-up names it. No version is inferred from a hash.

**Verdict.** The first sentence of the write-up's own verdict, conclusion, decision or outcome. "not recorded" means the write-up has none under those names; read its Result.

**Raw results.** Screenshots, logs and scripts are kept in the repository and are not published here. Personal data (home paths, device serials, addresses) was removed from the pages.

## Records 1 to 8: the first bugs and the Android immersive mode

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [1](/UScreen-Pro/experiments/1-bug-1-gui-missing-encoder-option/) | Bug 1 - GUI Missing Encoder Option | not recorded |
| [2](/UScreen-Pro/experiments/2-bug-2-system-freeze-on-tablet-plug-in/) | Bug 2 - System Freeze on Tablet Plug-in | not recorded |
| [3](/UScreen-Pro/experiments/3-bug-3-directional-lag-and-ghost-cursor-critical/) | Bug 3 - Directional Lag and Ghost Cursor (Critical) | not recorded |
| [4](/UScreen-Pro/experiments/4-bug-4-stubborn-meta-0-metavendor-monitor/) | Bug 4 - Stubborn Meta-0 (MetaVendor) Monitor | Meta-0 is completely eliminated from the live compositor layout and cannot return on subsequent boots because the systemd drop-in generating --virtual-monitor has been removed. |
| [5](/UScreen-Pro/experiments/5-scientific-cursor-handoff-boundary-synchronization/) | Scientific Cursor Handoff & Boundary Synchronization | not recorded |
| [7](/UScreen-Pro/experiments/7-android-immersive-ui-settings-overlay-concealment/) | Android Immersive UI & Settings Overlay Concealment | not recorded |
| [8](/UScreen-Pro/experiments/8-android-opt-in-immersive-ui-refactor/) | Android Opt-In Immersive UI Refactor | not recorded |

## Records 14 to 19: encoder, pacing and latency tuning

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [14](/UScreen-Pro/experiments/14-max-performance-mode-idle-60-fps-pacing/) | Max Performance Mode & Idle 60 FPS Pacing | The implementation fulfills all requirements: 1. powerprofilesctl is inspected non-blockingly via an asynchronous bounded query. |
| [16](/UScreen-Pro/experiments/16-system-wide-frame-pacing-rca-and-stabilization/) | System-Wide Frame Pacing RCA and Stabilization | By identifying and eliminating the FFmpeg CPU software filter bottleneck, pacing EVDI capture requests to 16.666ms, implementing anti-burst monotonic high-resolution ... |
| [17](/UScreen-Pro/experiments/17-latency-micro-optimization-stage-by-stage-profiling/) | Latency Micro-Optimization & Stage-by-Stage Profiling | not recorded |
| [18](/UScreen-Pro/experiments/18-advanced-low-latency-hevc-pipeline-tuning-empirical-comparison/) | Advanced Low-Latency HEVC Pipeline Tuning & Empirical Comparison | not recorded |
| [19](/UScreen-Pro/experiments/19-per-device-hardware-encoder-overrides-multi-device-architectural-routi/) | Per-Device Hardware Encoder Overrides & Multi-Device Architectural Routing | The daemon faithfully adhered to the fallback hierarchy: hevc_vaapi was invoked exclusively when explicitly overridden; upon removal, the pipeline fell back instantaneously to h264_vaapi. |

## Records 20 to 29: several tablets, connectivity and deployment

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [20](/UScreen-Pro/experiments/20-institutional-multi-device-ux-architecture-redesign-per-device-overrid/) | Institutional Multi-Device UX Architecture Redesign & Per-Device Override Engine | The multi-device UX overhaul, stream isolation engine, and per-device override system represent a major architectural leap forward for UScreen. |
| [21](/UScreen-Pro/experiments/21-uscreen-institutional-trio-features-gui-daemon-decoupling/) | UScreen Institutional Trio Features & GUI-Daemon Decoupling | Experiment 21 successfully achieves all four research and engineering objectives. |
| [25](/UScreen-Pro/experiments/25-institutional-root-cause-analysis-rca-isolated-multi-device-architectu/) | Institutional Root-Cause Analysis (RCA) & Isolated Multi-Device Architecture | The refactored isolated session management architecture, physical hardware deduplication, and graceful KMS teardown protocol completely eliminate all multi-device ... |
| [26](/UScreen-Pro/experiments/26-institutional-root-cause-analysis-rca-state-aware-ux-overhaul-and-adva/) | Institutional Root-Cause Analysis (RCA), State-Aware UX Overhaul, and Advanced... | All mandates of the engineering investigation have been achieved with academic rigor: 1. Root-Cause Analysis: The reconnection blackout, false onboarding instructions, ... |
| [27](/UScreen-Pro/experiments/27-institutional-multi-system-debugging-input-routing-power-management-an/) | Institutional Multi-System Debugging, Input Routing, Power Management, and Diagnostics... | 1. EDID Disambiguation Eliminates Mutter Cross-Talk: Modifying host/src/edid.rs to write serial_id.to_le_bytes() into bytes 12..16 guarantees distinct monitor identities in Mutter DisplayConfig (0x00000001 vs 0x00000002). |
| [28](/UScreen-Pro/experiments/28-final-deployment-working-directory-desktop-launcher-bug-rca-and-end-to/) | Final Deployment, Working Directory & Desktop Launcher Bug RCA, and End-to-End... | 1. Working Directory Resilience: Abstracting path lookup into find_apk_path() with canonical executable parent traversal and XDG data directory checking completely eliminates working-directory dependence. |
| [29](/UScreen-Pro/experiments/29-git-commit-id-build-stamping-dirty-build-detection-non-blocking-egui-i/) | Git Commit ID Build Stamping, Dirty Build Detection, Non-Blocking egui Installer... | All four operational mandates and hypotheses have been verified: 1. Hypothesis 1 (Deterministic Version Injection): Confirmed. |

## Records 33 to 50: the desktop app redesign and polish

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [33](/UScreen-Pro/experiments/33-institutional-multi-system-ux-architectural-overhaul-interactive-onboa/) | Institutional Multi-System UX & Architectural Overhaul: Interactive Onboarding,... | All requirements outlined in the institutional mandate have been satisfied and rigorously validated: 1. Interactive Intents: Replaced static instructions with direct ... |
| [34](/UScreen-Pro/experiments/34-institutional-architecture-for-secure-adb-pairing-android-11-mdns-dyna/) | Institutional Architecture for Secure ADB Pairing (Android 11+ mDNS), Dynamic Legacy... | All four stack targets (C helper, Rust daemon, egui frontend, Android Gradle APK) built cleanly with synchronized commit identifier v3511478-dirty. |
| [35](/UScreen-Pro/experiments/35-institutional-deep-codebase-cleansing-structural-modularization-and-pr/) | Institutional Deep Codebase Cleansing, Structural Modularization, and Pre-Tauri... | not recorded |
| [36](/UScreen-Pro/experiments/36-uscreen-pro-tauri-2-0-md3-rebirth-unified-build-system-qr-pairing-and/) | UScreen Pro — Tauri 2.0 MD3 Rebirth, Unified Build System, QR Pairing, and Jetpack... | The transition from the legacy egui frontend to Tauri 2.0 with React and Material Design 3 ("UScreen Pro") has elevated UScreen from an ad-hoc desktop utility to a ... |
| [37](/UScreen-Pro/experiments/37-uscreen-pro-institutional-ui-ux-overhaul-8-step-onboarding-wizard-qr-p/) | UScreen Pro — Institutional UI/UX Overhaul, 8-Step Onboarding Wizard, QR Pairing Fix,... | All five tasks mandated in the institutional UI overhaul have been completed and empirically verified: 1. Form Controls: Eradicated native <select> and checkbox white ... |
| [38](/UScreen-Pro/experiments/38-final-overhaul-audit-md3-ui-usb-bootstrapped-onboarding-forced-deploy/) | Final overhaul: audit, MD3 UI, USB-bootstrapped onboarding, forced deploy | not recorded |
| [39](/UScreen-Pro/experiments/39-frameless-window-light-dark-md3-daemon-controls-bug-fixes/) | Frameless window, light/dark MD3, daemon controls, bug fixes | not recorded |
| [41](/UScreen-Pro/experiments/41-final-polish-layout-add-device-modal-emerald-theme-app-icon/) | Final polish: layout, Add device modal, Emerald theme, app icon | not recorded |
| [42](/UScreen-Pro/experiments/42-broken-dropdowns-slider-scale-advanced-tab/) | Broken dropdowns, slider scale, Advanced tab | not recorded |
| [43](/UScreen-Pro/experiments/43-md3-expressive-audit-of-uscreen-pro-gui/) | MD3 Expressive audit of `uscreen-pro-gui` | not recorded |
| [44](/UScreen-Pro/experiments/44-migration-of-uscreen-pro-gui-to-shadcn-ui-md3-tokens-audit/) | Migration of `uscreen-pro-gui` to shadcn/ui + MD3 tokens: audit | not recorded |
| [46](/UScreen-Pro/experiments/46-ghost-cursor-tearing-placebo-settings-pause-usb-first-onboarding/) | Ghost cursor/tearing, placebo settings, Pause, USB-first onboarding | not recorded |
| [47](/UScreen-Pro/experiments/47-device-management-and-real-feedback/) | Device management and real feedback | not recorded |
| [48](/UScreen-Pro/experiments/48-phase-1-fixes-and-android-identity/) | Phase 1 fixes and Android identity | not recorded |
| [49](/UScreen-Pro/experiments/49-final-ui-logic-fixes-and-cleanup/) | Final UI logic fixes and cleanup | not recorded |
| [50](/UScreen-Pro/experiments/50-target-locking-and-ux-fixes/) | Target locking and UX fixes | not recorded |

## Records 51 to 66: transport, Wi-Fi setup, the EVDI freeze and power

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [51](/UScreen-Pro/experiments/51-touch-lock-setting-identify-heads-up-tablet-diagnosis-no-android-onboa/) | Touch-lock setting, Identify heads-up, tablet diagnosis, no Android onboarding, scroll | not recorded |
| [52](/UScreen-Pro/experiments/52-transport-order-android-safe-area-tablet-wireless-setup/) | Transport order, Android safe area, tablet wireless setup | not recorded |
| [53a](/UScreen-Pro/experiments/53a-dual-wireless-setup-automatic-usb-tailscale-with-empathetic-errors/) | Dual wireless setup (Automatic USB + Tailscale) with empathetic errors | not recorded |
| [53b](/UScreen-Pro/experiments/53b-setup-errors-host-offline-debugging-disabled/) | Setup errors, Host Offline, Debugging Disabled | not recorded |
| [56](/UScreen-Pro/experiments/56-rca-login-freeze-of-gnome-mutter-46-2-with-the-uscreen-daemon-evdi/) | RCA: login freeze of GNOME (mutter 46.2) with the UScreen daemon / EVDI | not recorded |
| [58](/UScreen-Pro/experiments/58-six-gui-latency-bugs/) | Six GUI / latency bugs | not recorded |
| [60](/UScreen-Pro/experiments/60-codebase-cleansing-and-upstream-sync/) | Codebase cleansing and upstream sync | not recorded |
| [64](/UScreen-Pro/experiments/64-power-management-overridable-settings-and-state-sync/) | Power management, overridable settings and state sync | - Touchpad mode replaces the old tablet-only touchpad switch. |
| [66](/UScreen-Pro/experiments/66-settings-reorganisation-exclusive-input-modes-curve-editor-brightness/) | Settings reorganisation, exclusive input modes, curve editor, brightness fixes | not recorded |

## Records 69 to 76: the input engine, the touchpad and stylus research

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [69](/UScreen-Pro/experiments/69-stylus-profiles-touchpad-gestures-haptics-warnings/) | Stylus profiles, touchpad gestures, haptics, warnings | - S Pen SDK is vendored, not a dependency. |
| [71](/UScreen-Pro/experiments/71-touchpad-shape-native-gestures-multi-finger-taps-s-pen-debugging/) | Touchpad shape, native gestures, multi-finger taps, S Pen debugging | not recorded |
| [72](/UScreen-Pro/experiments/72-input-engine-rewrite/) | Input engine rewrite | not recorded |
| [73](/UScreen-Pro/experiments/73-input-engine-rewrite-v2-based-on-proven-libraries/) | Input engine rewrite v2, based on proven libraries | not recorded |
| [75](/UScreen-Pro/experiments/75-native-continuous-touchpad-gestures-on-linux-research-only/) | Native, continuous touchpad gestures on Linux (research only) | not recorded |
| [76](/UScreen-Pro/experiments/76-native-virtual-touchpad-raw-fingers-to-libinput/) | Native virtual touchpad (raw fingers to libinput) | not recorded |

## Records 81 to 103: stylus handling, ghost tracking and the pen menu

| ID | Experiment | Verdict |
| :--- | :--- | :--- |
| [81](/UScreen-Pro/experiments/81-comprehensive-stylus-architecture-research-linux-gnome-46-wayland/) | Comprehensive stylus architecture research (Linux, GNOME 46 / Wayland) | not recorded |
| [88](/UScreen-Pro/experiments/88-touch-jump-laggy-pointer-and-finicky-clicks-on-the-native-virtual-touc/) | Touch jump, laggy pointer and finicky clicks on the native virtual touchpad | not recorded |
| [90](/UScreen-Pro/experiments/90-device-settings-ui-fix-and-the-sensitivity-slider/) | Device Settings UI fix and the Sensitivity slider | not recorded |
| [95](/UScreen-Pro/experiments/95-universal-ghost-tracking-android-pen-physical-drawing-tablets/) | Universal ghost tracking (Android pen + physical drawing tablets) | not recorded |
| [96](/UScreen-Pro/experiments/96-fix-unified-ghost-tracking-clicks-dropped-invisible-physical-cursor-st/) | Fix unified ghost tracking (clicks dropped, invisible physical cursor, stuck button) | not recorded |
| [97](/UScreen-Pro/experiments/97-auto-mode-as-one-unified-pen-android-only/) | Auto mode as one unified pen (Android only) | not recorded |
| [98](/UScreen-Pro/experiments/98-input-state-machine-audit-android-auto-mode/) | Input state machine audit (Android auto mode) | not recorded |
| [99](/UScreen-Pro/experiments/99-fix-unresponsive-pen-tip-and-dropped-drag-select-follow-up-to-98/) | Fix unresponsive pen tip and dropped drag-select (follow-up to 98) | not recorded |
| [100](/UScreen-Pro/experiments/100-resting-pen-no-longer-takes-the-cursor-hover-park-filter-pen-recorder/) | Resting pen no longer takes the cursor (hover park filter) + pen recorder | not recorded |
| [101](/UScreen-Pro/experiments/101-click-slop-a-tap-on-selected-text-no-longer-starts-a-drag/) | Click slop (a tap on selected text no longer starts a drag) | not recorded |
| [102](/UScreen-Pro/experiments/102-codebase-cleanup-the-desktop-layer-and-the-pen-s-long-press-menu/) | Codebase cleanup, the desktop layer, and the pen's long-press menu | not recorded |
| [103](/UScreen-Pro/experiments/103-capturing-the-real-scanout-to-see-the-ghost-cursor-dropped/) | Capturing the real scanout to see the ghost cursor (dropped) | do not use kmsgrab for checks. It needs root, does not give a readable picture on this GPU, |

## Sources

- `experiments_and_findings/` (the write-ups and their evidence folders)
- `website/scripts/convert-experiments.mjs` (the conversion)
