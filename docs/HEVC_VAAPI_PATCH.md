# HEVC VA-API Hardware Encoder Patch

## Summary

This patch enables hardware-accelerated HEVC (H.265) encoding via VA-API (`hevc_vaapi`) in UScreen, provides proper argument and rate-control configuration for ffmpeg, and implements a graceful fallback mechanism to `h264_vaapi` should `hevc_vaapi` fail to initialize or encounter runtime streaming errors.

---

## Why Was This Changed?

Modern Intel systems (such as Intel Meteor Lake with Intel Arc Graphics / Xe-LPG architecture) and modern AMD GPUs provide full hardware HEVC encode capabilities through VA-API (`VAProfileHEVCMain` and `VAProfileHEVCMain10`, verifiable via `vainfo`).

Connected clients such as the Samsung Galaxy Tab S9 FE (`SM-X616B`) support hardware HEVC decoding. Using HEVC provides superior compression efficiency, lower USB bandwidth usage, and sharper image/text quality compared to H.264 at identical bitrates.

Previously, configuring `encoder = "hevc_vaapi"` in `~/.config/uscreen/config.toml` caused UScreen to crash repeatedly with:
```
ERROR uscreen::capture: Failed to start encoder: Unknown encoder: hevc_vaapi. Use h264_nvenc, hevc_nvenc, h264_vaapi, or libx264.
```

---

## What Was Changed?

### 1. `host/src/capture/encoders.rs`
- **Candidate List**: Expanded `CANDIDATES` array size from 3 to 4 and added `"hevc_vaapi"`:
  ```rust
  pub const CANDIDATES: [&str; 4] = ["h264_nvenc", "h264_vaapi", "hevc_vaapi", "libx264"];
  ```
- **Graceful Fallback in Resolution**: Updated `encoders::resolve()` so that if `configured == "hevc_vaapi"` fails a probe frame test, it automatically checks if `"h264_vaapi"` works and falls back to it with a warning log rather than leaving the daemon unconfigured or failing.
- **Unit Test Coverage**: Added test coverage in `vaapi_setup_names_the_chosen_render_node` for `"hevc_vaapi"`.

### 2. `host/src/capture.rs`
- **Validation Gate**: Added `"hevc_vaapi"` to the allowed encoder match statement:
  ```rust
  let res: Result<()> = match encoder {
      "h264_nvenc" | "hevc_nvenc" | "h264_vaapi" | "hevc_vaapi" | "libx264" => Ok(()),
      _ => anyhow::bail!(
          "Unknown encoder: {}. Use h264_nvenc, hevc_nvenc, h264_vaapi, hevc_vaapi, or libx264",
          encoder
      ),
  };
  res?;
  ```
- **VA-API Parameter Setup**: Included `hevc_vaapi` alongside `h264_vaapi` for:
  - `-vaapi_device <device>`
  - `-vf format=nv12,hwupload`
  - Constant Quality rate control parameters (`-rc_mode CQP -qp <quality> -maxrate <bitrate>k -bf 0 -g <gop> -idr_interval 1`).
- **Graceful Runtime Fallback**:
  - In `start_encoder()`: If `hevc_vaapi` fails during process spawn or initialization, a warning is logged and the daemon automatically switches `self.config.encoder = "h264_vaapi"` and spawns the fallback encoder rather than repeatedly crash-looping.
  - In `stream_frames()`: If the encoder child process terminates with an error during streaming and `self.config.encoder == "hevc_vaapi"`, UScreen logs a warning and falls back to `"h264_vaapi"` on restart.
- **Unit Test Coverage**: Added `allowed_encoders_include_hevc_vaapi` test.

---

## How to Revert

### Using Git:
If you are on the `feature/hevc-vaapi-and-mutter-fix` branch or have merged it:
```bash
# Revert the specific commit:
git revert <commit-hash>

# Or reset local changes to main:
git checkout main
```

### Manual Reversion:
1. In `host/src/capture/encoders.rs`:
   - Change `pub const CANDIDATES: [&str; 4]` back to:
     ```rust
     pub const CANDIDATES: [&str; 3] = ["h264_nvenc", "h264_vaapi", "libx264"];
     ```
   - Remove `"hevc_vaapi"` checks from `resolve()`.
2. In `host/src/capture.rs`:
   - Remove `"hevc_vaapi"` from the match gate and VA-API argument checks.
3. Rebuild and reinstall:
   ```bash
   cargo build --release
   sudo cp target/release/uscreen /usr/bin/uscreen
   systemctl --user restart uscreen
   ```
