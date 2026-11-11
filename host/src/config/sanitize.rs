//! `FileConfig::sanitize`: pulls a freshly loaded (or tablet-pushed) config back into the ranges the
//! pipeline can serve and migrates old keys. Owns validation only; the shape of the config is in
//! `mod.rs`, the allowed ranges in `limits`, the text normalisers in `clean`.

use super::*;

impl FileConfig {
    /// Pull persisted values back into the range the pipeline can actually
    /// serve. Older builds let the tablet push 200 Mbps @ 90 fps and wrote it
    /// straight to disk, so existing installs carry settings that guarantee
    /// multi-second latency until they are clamped here.
    pub fn sanitize(&mut self) {
        let bitrate = self.bitrate.clamp(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS);
        if bitrate != self.bitrate {
            tracing::warn!(
                "Bitrate {} kbps is beyond what the USB transport sustains — clamped to {} kbps",
                self.bitrate,
                bitrate
            );
            self.bitrate = bitrate;
        }

        let fps = self.fps.clamp(MIN_FPS, MAX_FPS);
        if fps != self.fps {
            tracing::warn!("fps {} out of range — clamped to {}", self.fps, fps);
            self.fps = fps;
        }

        self.quality = self.quality.clamp(MIN_QUALITY, MAX_QUALITY);
        self.stream_scale = self.stream_scale.clamp(1, 4);
        self.max_tablets = self.max_tablets.clamp(1, 4);
        self.width = self.width.clamp(640, 8192);
        self.height = self.height.clamp(480, 8192);
        self.idle_dim_timeout_secs = self
            .idle_dim_timeout_secs
            .clamp(MIN_IDLE_TIMEOUT_SECS, MAX_IDLE_TIMEOUT_SECS);
        self.touchpad_sensitivity = self.touchpad_sensitivity.clamp(50, 200);
        self.dim_duration_ms = self.dim_duration_ms.clamp(MIN_FADE_MS, MAX_FADE_MS);
        self.undim_duration_ms = self.undim_duration_ms.clamp(MIN_FADE_MS, MAX_FADE_MS);
        self.curve_presets.retain(|p| !p.name.trim().is_empty());
        for p in &mut self.curve_presets {
            p.name = p.name.trim().to_string();
            p.x1 = p.x1.clamp(0.0, 1.0);
            p.x2 = p.x2.clamp(0.0, 1.0);
            p.y1 = p.y1.clamp(BEZIER_Y_MIN, BEZIER_Y_MAX);
            p.y2 = p.y2.clamp(BEZIER_Y_MIN, BEZIER_Y_MAX);
        }
        for (curve, fallback) in [
            (&mut self.dim_curve, "ease_out"),
            (&mut self.undim_curve, "ease_in"),
        ] {
            // A `preset:` reference stays even when its preset was deleted: it resolves to the
            // default wherever it is used. Everything else is normalised here.
            if !curve.starts_with("preset:") {
                *curve = resolve_curve(curve, &self.curve_presets, fallback);
            }
        }
        self.orientation = match self.orientation.trim().to_ascii_lowercase().as_str() {
            o @ ("auto" | "camera_up" | "camera_down") => o.to_string(),
            _ => "auto".into(),
        };
        self.theme_mode = match self.theme_mode.trim().to_ascii_lowercase().as_str() {
            t @ ("system" | "light" | "dark") => t.to_string(),
            _ => "system".into(),
        };
        self.auto_open_app = clean_auto_open(&self.auto_open_app);
        self.right_click_method = clean_right_click(&self.right_click_method);
        self.touchpad_shape = clean_shape(&self.touchpad_shape);
        self.stylus_style = clean_stylus_style(&self.stylus_style);
        self.stylus_button_primary = clean_pen_button(&self.stylus_button_primary, "right");
        self.stylus_button_secondary = clean_pen_button(&self.stylus_button_secondary, "middle");
        self.pen_menu_style = clean_menu_style(&self.pen_menu_style);
        self.pen_menu_hold_ms = self.pen_menu_hold_ms.clamp(500, 1500);
        self.pen_click_hold_ms = self.pen_click_hold_ms.min(300);
        self.stylus_art_apps = self
            .stylus_art_apps
            .iter()
            .map(|a| a.trim().to_ascii_lowercase())
            .filter(|a| !a.is_empty())
            .collect();
        self.physical_tablet_device = self.physical_tablet_device.trim().to_ascii_lowercase();
        for p in self.tablets.values_mut() {
            if let Some(m) = p.stylus_style.take() {
                p.stylus_style = Some(clean_stylus_style(&m));
            }
            if let Some(m) = p.stylus_button_primary.take() {
                p.stylus_button_primary = Some(clean_pen_button(&m, "right"));
            }
            if let Some(m) = p.stylus_button_secondary.take() {
                p.stylus_button_secondary = Some(clean_pen_button(&m, "middle"));
            }
            if let Some(m) = p.right_click_method.take() {
                p.right_click_method = Some(clean_right_click(&m));
            }
            if let Some(m) = p.touchpad_shape.take() {
                p.touchpad_shape = Some(clean_shape(&m));
            }
        }
        // One-time migration from the two booleans this enum replaced.
        if self.input_mode.is_none() {
            self.input_mode = Some(if self.touchpad_mode {
                InputMode::Touchpad
            } else if self.pen_only {
                InputMode::Stylus
            } else {
                InputMode::Display
            });
        }
        self.touchpad_mode = false;
        self.pen_only = self.input_mode == Some(InputMode::Stylus);
        for p in self.tablets.values_mut() {
            if p.input_mode.is_none() && p.touchpad_mode == Some(true) {
                p.input_mode = Some(InputMode::Touchpad);
            }
            p.touchpad_mode = None;
        }

        if Position::parse(&self.position).is_none() {
            tracing::warn!(
                "Unknown position {:?} — falling back to \"right\"",
                self.position
            );
            self.position = "right".into();
        }
    }
}
