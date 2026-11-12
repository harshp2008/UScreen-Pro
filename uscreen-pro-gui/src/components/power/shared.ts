/*
 * Types and small helpers shared by the power-settings panels: the settings both global and per device, the props every panel takes,
 * and the builder of the "override the global value" switch.
 */
import type { InputMode } from "../../lib/types";
import type { Override } from "../ui/layout";

/** The settings that exist both globally and as a per-device override; every one is also on the tablet. */
export interface PowerValues {
  input_mode: InputMode;
  touch_input: boolean;
  blank_pen_only: boolean;
  immersive_mode: boolean;
  show_stats: boolean;
  orientation: string;
  theme_mode: string;
  keep_connected_in_background: boolean;
  keep_display_alive_when_locked: boolean;
  touch_on_lock_screen: boolean;
  battery_saver_on_disconnect: boolean;
  battery_saver_lock: boolean;
  idle_dim_enabled: boolean;
  idle_dim_timeout_secs: number;
  dim_curve: string;
  undim_curve: string;
  dim_duration_ms: number;
  undim_duration_ms: number;
  touchpad_haptics: boolean;
  touchpad_haptic_strength: string;
  touchpad_click_sound: boolean;
  touchpad_style: string;
  touchpad_middle_button: boolean;
  touchpad_classic_tap: boolean;
  native_gestures: boolean;
  touchpad_sensitivity: number;
  right_click_method: string;
  touchpad_shape: string;
  stylus_style: string;
  stylus_button_primary: string;
  stylus_button_secondary: string;
  pen_menu: boolean;
  pen_menu_style: string;
  pen_menu_hold_ms: number;
}

/** Tooltip of the Input mode row of a device: where the power goes and what to do about it. */
export const POWER_TIP =
  "Streaming at high resolutions/framerates consumes significant power. If your device charges slowly, try lowering the FPS, or use Touchpad Mode.";

/** Text for a boolean setting. */
export const onOff = (v: boolean) => (v ? "On" : "Off");

/**
 * Props shared by the power-settings sections: effective values, a change callback, and on device pages a builder of the override switch.
 */
export interface Common {
  /** Effective values: the global ones on the Settings page, override-or-global on a device page. */
  values: PowerValues;
  onChange: (patch: Partial<PowerValues>) => void;
  /** Present on a device page: builds the "override the global value" switch of a row. */
  override?: (key: keyof PowerValues, inherited: string) => Override;
}

/** The override switch builder for a row, if the page supports overrides. */
export const overrideOf = (c: Common) => (key: keyof PowerValues, inherited: string) => c.override?.(key, inherited);
