# Experiment & Finding 19: Per-Device Hardware Encoder Overrides & Multi-Device Architectural Routing

**Author**: Principal Systems Engineer & Open-Source Maintainer  
**Date**: November 5, 2026  
**Document Status**: Formal Academic Systems Engineering Specification & Laboratory Journal  
**Target Host**: Intel Core Ultra 7 (Meteor Lake-P / Intel Arc Xe-LPG) | Ubuntu 24.04 LTS (Kernel 7.0.0-generic) | GNOME 46.0 Wayland  
**Target Client(s)**: Samsung Galaxy Tab S9 FE (`SM-X616B`, Exynos 1380 SoC, 2560x1600 @ 90Hz) & Multi-Device Topologies  
**Branch**: `feat/per-device-encoder-overrides`  

---

## 1. Abstract & Architectural Motivation

In modern multi-display workspaces, users frequently connect heterogeneous Android devices simultaneously (for instance, a high-performance primary tablet such as a Samsung Galaxy Tab S9 FE capable of hardware HEVC alongside a secondary or older smartphone limited to H.264 Baseline/Main profile). Historically, UScreen maintained a monolithic global `encoder` configuration across all devices. If the user configured `hevc_vaapi`, legacy devices lacking HEVC capability suffered decoder initialization panics or total stream blackouts; conversely, constraining the host to `h264_vaapi` penalized modern hardware by forfeiting the bandwidth compression efficiency and 10-bit color precision of HEVC.

To resolve this architectural limitation, we implement **Per-Device Encoder Overrides** across the Rust daemon backend and graphical configuration interface:
1. **Per-Device Profile Extension**: Extended `TabletProfile` in `host/src/config.rs` with an optional `encoder: Option<String>` field mapped under `[tablets.<serial>]` in `~/.config/uscreen/config.toml`.
2. **Hierarchical Dynamic Routing**: Modified session spawning (`spawn_extra_session`) and runtime synchronization (`apply_tablet_profile`) in `host/src/main.rs`. When a device connects, its pipeline resolves the per-device override if explicitly configured; if `None`, it smoothly falls back to the global `encoder` default.
3. **Session Invariant Protection**: Guarded runtime persistence (`save_handle`) so that active device overrides never overwrite or corrupt the global default setting in `config.toml`.
4. **Dynamic Bitstream WebSocket Negotiation**: Updated `host/src/input.rs` so that connecting tablet WebSocket clients dynamically receive the exact codec bitstream format (`"hevc"` or `"h264"`) corresponding to their active encoder pipeline, preventing MediaCodec format mismatches.
5. **Multi-Device GUI Controls**: Overhauled `gui/src/main.rs` to detect all connected and saved devices via ADB, rendering a dedicated dropdown for each tablet defaulting to `"Follow Global Setting (<current global>)"` alongside individual hardware encoder targets (`AMD / Intel H.264`, `AMD / Intel HEVC`, `NVIDIA H.264`, `NVIDIA HEVC`, `CPU libx264`).
6. **Accessibility Runtime Stabilization**: Eliminated an upstream `eframe`/`accesskit_unix`/`zbus` Tokio reactor panic in `uscreen-gui` by configuring `default-features = false` and scoping native Wayland/X11 rendering without headless DBus executor deadlocks.

---

## 2. Architectural Design & Implementation

```
                                  +---------------------------+
                                  |   ~/.config/uscreen/      |
                                  |       config.toml         |
                                  +-------------+-------------+
                                                |
                       +------------------------+------------------------+
                       | Global: encoder = "h264_vaapi"                  |
                       | [tablets.R52X...]: encoder = "hevc_vaapi"       |
                       | [tablets.OLD_DEV]: (encoder unset -> None)      |
                       +------------------------+------------------------+
                                                |
                       +------------------------+------------------------+
                       |            UScreen Host Daemon                  |
                       +------------------------+------------------------+
                                                |
               +--------------------------------+--------------------------------+
               | Primary Session (Instance 0)                                     | Secondary Session (Instance 1)
               | Device: TABLET_SERIAL                                             | Device: OLD_DEV_SERIAL
               | Effective Encoder: hevc_vaapi                                   | Effective Encoder: h264_vaapi (Fallback)
               v                                                                 v
+-------------------------------+                                 +-------------------------------+
| FFmpeg Process                |                                 | FFmpeg Process                |
| -c:v hevc_vaapi               |                                 | -c:v h264_vaapi               |
| -profile:v main               |                                 | -rc_mode CQP -qp 17           |
| -b_depth 1 -sei 0 -aud 0      |                                 | -async_depth 1                |
+---------------+---------------+                                 +---------------+---------------+
                |                                                                 |
                v                                                                 v
+-------------------------------+                                 +-------------------------------+
| WebSocket Negotiation         |                                 | WebSocket Negotiation         |
| codec: "hevc"                 |                                 | codec: "h264"                 |
+---------------+---------------+                                 +---------------+---------------+
                |                                                                 |
                v                                                                 v
+-------------------------------+                                 +-------------------------------+
| Samsung Galaxy Tab S9 FE      |                                 | Secondary Tablet / Phone      |
| Hardware MediaCodec: HEVC     |                                 | Hardware MediaCodec: H.264    |
+-------------------------------+                                 +-------------------------------+
```

### 2.1 Backend Configuration & Routing ([`host/src/config.rs`](file:///home/harsh/Documents/GITHUB PROJECTS/UScreen/host/src/config.rs), [`host/src/main.rs`](file:///home/harsh/Documents/GITHUB PROJECTS/UScreen/host/src/main.rs))

In `host/src/config.rs`, `TabletProfile` incorporates `encoder`:
```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct TabletProfile {
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub width_mm: u32,
    pub height_mm: u32,
    pub fps: Option<u32>,
    pub bitrate: Option<u32>,
    pub quality: Option<u32>,
    pub stream_scale: Option<u32>,
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
}
```

In `host/src/main.rs`, `apply_tablet_profile` evaluates per-device overrides:
```rust
async fn apply_tablet_profile(serial: &str, settings_tx: &watch::Sender<capture::EncoderSettings>) {
    let mut cfg = config::FileConfig::load();
    let Some(p) = cfg.tablets.get(serial).cloned() else { ... };
    let mut s = settings_tx.borrow().clone();
    let before = (s.width, s.height, s.width_mm, s.height_mm, s.fps, s.bitrate, s.quality, s.stream_scale, s.encoder.clone());
    ...
    if let Some(ref enc) = p.encoder {
        if !enc.trim().is_empty() {
            s.encoder = enc.clone();
        } else {
            s.encoder = cfg.encoder.clone();
        }
    } else {
        s.encoder = cfg.encoder.clone();
    }
    if before == (s.width, s.height, s.width_mm, s.height_mm, s.fps, s.bitrate, s.quality, s.stream_scale, s.encoder.clone()) {
        return;
    }
    info!(
        "Tablet profile applied for {}{}: {}x{} ({}x{} mm), encoder: {} ({})",
        serial,
        if p.label.is_empty() { String::new() } else { format!(" ({})", p.label) },
        s.width, s.height, s.width_mm, s.height_mm,
        s.encoder,
        if p.encoder.is_some() { "device override" } else { "global default" }
    );
    let _ = settings_tx.send(s);
}
```

In `host/src/main.rs`, session template instantiation dynamically resolves codecs for multi-tablet instances:
```rust
fn spawn_extra_session(t: &ExtraSessionTemplate, instance: u32, serial: &str) -> Option<ExtraSession> {
    let cards = vdisplay::evdi_cards();
    let mut cfg = t.cap_template.clone();
    cfg.instance = instance;
    let file_cfg = config::FileConfig::load();
    if let Some(p) = file_cfg.tablets.get(serial) {
        if let Some(ref enc) = p.encoder {
            if !enc.trim().is_empty() {
                cfg.encoder = enc.clone();
            }
        }
    }
    let codec = capture::Codec::from_encoder(&cfg.encoder).muxer().to_string();
    ...
```

### 2.2 Dynamic WebSocket Protocol Sync ([`host/src/input.rs`](file:///home/harsh/Documents/GITHUB PROJECTS/UScreen/host/src/input.rs))

```rust
    let current_codec = |tx: &Option<watch::Sender<EncoderSettings>>, fallback: &str| -> String {
        tx.as_ref()
            .map(|t| crate::capture::Codec::from_encoder(&t.borrow().encoder).muxer().to_string())
            .unwrap_or_else(|| fallback.to_string())
    };
    let current_settings = |tx: &Option<watch::Sender<EncoderSettings>>| {
        tx.as_ref().map(|t| {
            let s = t.borrow();
            (s.fps, s.bitrate, s.encoder.clone())
        })
    };
```

---

## 3. Empirical Verification & Unedited Execution Evidence

### 3.1 Test Case 1: Active Device Override (`hevc_vaapi`) with Global Default (`h264_vaapi`)

**Configuration**:
```toml
encoder = "h264_vaapi"
fps = 90
bitrate = 20000
width = 2560
height = 1600
quality = 17

[tablets.TABLET_SERIAL]
label = "SM-X616B"
width = 2560
height = 1600
width_mm = 266
height_mm = 166
encoder = "hevc_vaapi"
```

**Empirical Process Inspection (`ps aux | grep ffmpeg`)**:
```
harsh  346943 158  0.7 3793956 181644 ? RNLl 01:20 0:02 ffmpeg -hide_banner -vaapi_device /dev/dri/renderD128 -fflags nobuffer -flags low_delay -color_primaries bt709 -color_trc bt709 -colorspace bt709 -color_range tv -f rawvideo -pix_fmt nv12 -s 2560x1600 -framerate 90 -i /run/user/1000/uscreen/capture.fifo -vf format=nv12,hwupload -c:v hevc_vaapi -rc_mode CQP -qp 17 -maxrate 20000k -bf 0 -g 90 -idr_interval 1 -async_depth 1 -profile:v main -b_depth 1 -sei 0 -aud 0 -f hevc pipe:1
```

**Real-Time Journal Logs (`journalctl --user -u uscreen.service`)**:
```
Nov 05 01:20:02 harsh uscreen[345889]: 2026-11-04T19:50:02.449496Z  INFO uscreen::capture: 1586: Encoder: 444 access units in 5.0s, 0.2 MB/s (1329 kbps)
Nov 05 01:20:02 harsh uscreen[346006]: [evdi-helper] 89.7 grabs/s (total 813), mode:1 dpms:1 pending:0
Nov 05 01:20:02 harsh uscreen[346006]: [evdi-helper] stages: req→ready avg 6.3ms (391 waited, 58 imm), grab avg 0.1ms, conv avg 0.3ms, write avg 4.0ms
Nov 05 01:20:02 harsh uscreen[346006]: [evdi-helper] capture→fifo p50 7.4ms p95 9.3ms (256 frames)
Nov 05 01:20:03 harsh uscreen[345889]: 2026-11-04T19:50:03.106683Z  INFO uscreen::latency: 179: Latency encode→display: p50 20.8ms  p95 23.4ms  max 33.0ms  (445 samples, 2 in flight, 0 aged out)
Nov 05 01:20:03 harsh uscreen[345889]: 2026-11-04T19:50:03.106724Z  INFO uscreen::latency: 194:   of which tablet decode+render p50 16.1ms  p95 18.5ms  → wire ~4.4ms
```

### 3.2 Test Case 2: Override Fallback to Global Default (`h264_vaapi`)

**Configuration**:
`encoder` removed from `[tablets.TABLET_SERIAL]`. Global `encoder = "h264_vaapi"`.

**Empirical Process Inspection (`ps aux | grep ffmpeg`)**:
```
harsh  346494 163  0.3 3652532 94944 ? RNLl 01:20 0:02 ffmpeg -hide_banner -vaapi_device /dev/dri/renderD128 -fflags nobuffer -flags low_delay -color_primaries bt709 -color_trc bt709 -colorspace bt709 -color_range tv -f rawvideo -pix_fmt nv12 -s 2560x1600 -framerate 90 -i /run/user/1000/uscreen/capture.fifo -vf format=nv12,hwupload -c:v h264_vaapi -rc_mode CQP -qp 17 -maxrate 20000k -bf 0 -g 90 -idr_interval 1 -async_depth 1 -f h264 pipe:1
```

**Conclusion**: The daemon faithfully adhered to the fallback hierarchy: `hevc_vaapi` was invoked exclusively when explicitly overridden; upon removal, the pipeline fell back instantaneously to `h264_vaapi`.

---

## 4. Automated Test Suite Verification

```
$ cargo test
   Compiling uscreen v1.2.12 (/home/harsh/Documents/GITHUB PROJECTS/UScreen/host)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.94s
     Running unittests src/main.rs (target/debug/deps/uscreen-3199b95ec9845174)

running 39 tests
test capture::tests::allowed_encoders_include_hevc_vaapi ... ok
test capture::tests::codec_is_picked_from_the_encoder_name ... ok
test capture::tests::evdi_problem_reports_a_missing_module ... ok
test capture::tests::hevc_cra_counts_as_a_join_point ... ok
test capture::tests::hevc_parameter_sets_and_keyframes_are_recognised ... ok
test capture::tests::evdi_problem_reports_a_module_with_no_devices ... ok
test capture::tests::hevc_splits_pictures_on_the_first_slice_flag ... ok
test capture::tests::evdi_problem_stays_quiet_when_a_device_exists ... ok
test capture::tests::packetizer_does_not_emit_partial_nals ... ok
test capture::tests::packetizer_handles_start_code_split_across_reads ... ok
test capture::tests::packetizer_prepends_sps_pps_to_idr ... ok
test capture::tests::packetizer_splits_multiple_access_units_in_one_buffer ... ok
test cosmic::tests::reads_output_state_with_and_without_colour_codes ... ok
test doctor::tests::reads_the_kwin_version_from_its_banner ... ok
test edid::tests::edid_checksum_is_valid ... ok
test encoders::tests::skips_nvidia_and_keeps_node_order ... ok
test encoders::tests::vaapi_setup_names_the_chosen_render_node ... ok
test gnome::tests::builds_the_per_device_schema_path_and_value ... ok
test hyprland::tests::device_keys_match_hyprctl_devices ... ok
test gnome::tests::reads_the_edid_triple_from_getcurrentstate_json ... ok
test hyprland::tests::scale_is_whole_and_tablet_sized ... ok
test hyprland::tests::reads_an_output_hyprland_left_off ... ok
test input::tests::the_greeting_carries_the_computers_stream_settings_when_it_has_them ... ok
test input::tests::a_touching_pen_never_reports_zero_pressure ... ok
test kwin::tests::is_kde_session_reads_the_desktop_list ... ok
test kwin::tests::busctl_string_array_is_split_on_quotes ... ok
test input::tests::pen_lift_sends_its_position_before_leaving_proximity ... ok
test update::tests::tag_is_read_from_release_json ... ok
test kwin::tests::busctl_scalar_drops_the_signature_and_quotes ... ok
test input::tests::eraser_lift_leaves_proximity_with_the_eraser_tool ... ok
test x11::tests::finds_our_output_by_edid_not_by_name ... ok
test runtime::tests::runtime_dir_is_private ... ok
test tests::tells_network_serials_from_usb_ones ... ok
test x11::tests::falls_back_to_the_suffixed_name_without_an_edid ... ok
test runtime::tests::token_comparison_is_exact ... ok
test x11::tests::matches_a_device_and_its_sub_devices_but_not_the_next_tablet ... ok
test update::tests::version_comparison ... ok
test x11::tests::reads_outputs_and_their_edid_from_xrandr_prop ... ok
test config::tests::per_device_encoder_deserialization_and_serialization ... ok

test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```
