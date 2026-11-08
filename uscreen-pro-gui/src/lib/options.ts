/** A selectable value and its label for the settings dropdowns; values are strings because they come from `<select>`. */
export interface Option {
  value: string;
  label: string;
}

/** ffmpeg encoders the host can use; the daemon probes them and falls back if one does not work on this machine. */
export const ENCODERS: Option[] = [
  { value: "h264_nvenc", label: "NVIDIA H.264 (NVENC)" },
  { value: "hevc_nvenc", label: "NVIDIA HEVC (NVENC)" },
  { value: "h264_vaapi", label: "Intel / AMD H.264 (VAAPI)" },
  { value: "hevc_vaapi", label: "Intel / AMD HEVC (VAAPI)" },
  { value: "libx264", label: "Software (libx264)" },
];

/** Selectable frame rates. The host caps the virtual display at 90 Hz, so higher values are clamped there. */
export const FPS: Option[] = [
  { value: "30", label: "30 fps" },
  { value: "60", label: "60 fps" },
  { value: "90", label: "90 fps" },
  { value: "120", label: "120 fps" },
];

/** Which edge of the existing desktop the virtual screen is placed against. */
export const POSITIONS: Option[] = [
  { value: "left", label: "Left" },
  { value: "right", label: "Right" },
  { value: "above", label: "Above" },
  { value: "below", label: "Below" },
];

/** Stream scale: 1 sends every pixel, 2 halves each dimension to save bandwidth. */
export const STREAM_DETAIL: Option[] = [
  { value: "1", label: "Full" },
  { value: "2", label: "Half" },
];

/** Which adb transport a device may use: USB only, Wi-Fi only, or USB first with Wi-Fi as fallback. */
export const TRANSPORTS: Option[] = [
  { value: "usb", label: "USB cable" },
  { value: "auto", label: "Auto (USB first, Wi-Fi fallback)" },
  { value: "wifi", label: "Wi-Fi" },
];

/** Label of the option whose value matches, or the raw value when none does. */
export function labelOf(options: Option[], value: string | number | undefined): string {
  return options.find((o) => o.value === String(value))?.label ?? String(value ?? "");
}

/** Encoder CRF/QP scale of the quality slider: lower is sharper. 18 is the host default. */
export const QUALITY_MIN = 12;
export const QUALITY_MAX = 32;
export const DEFAULT_QUALITY = 18;

/** Named band for a CRF value, shown next to the number while the slider moves. */
export function qualityLabel(crf: number): string {
  if (crf <= 15) return "Maximum sharpness";
  if (crf <= 20) return "Balanced";
  if (crf <= 25) return "Efficient";
  return "Low bandwidth";
}

/** Fade curves of the idle dimming; the values are the names the host sends to the tablet app. */
export const CURVES: Option[] = [
  { value: "linear", label: "Linear" },
  { value: "ease_in", label: "Ease in" },
  { value: "ease_out", label: "Ease out" },
  { value: "ease_in_out", label: "Ease in-out" },
  { value: "exponential", label: "Exponential" },
];

/** Bounds of the idle-dim sliders; kept in step with `config.rs` on the host. */
export const IDLE_TIMEOUT_MIN = 10;
export const IDLE_TIMEOUT_MAX = 300;
export const FADE_MIN_MS = 100;
export const FADE_MAX_MS = 30_000;

/** The three mutually exclusive ways a tablet can be used. */
export const INPUT_MODES: Option[] = [
  { value: "display", label: "Extended Display" },
  { value: "touchpad", label: "Touchpad" },
  { value: "stylus", label: "Stylus (Graphics Tablet)" },
];

/** What each input mode does, shown under the selector. */
export const INPUT_MODE_HELP: Record<string, string> = {
  display: "A second screen: video is streamed to the tablet and touch is absolute.",
  touchpad: "A laptop touchpad: no video is streamed, so USB bandwidth and battery are saved.",
  stylus: "A graphics tablet: no video is streamed; the pen draws on this computer's own screen.",
};

/** Landscape direction the tablet app pins itself to. */
export const ORIENTATIONS: Option[] = [
  { value: "auto", label: "Auto" },
  { value: "camera_up", label: "Camera up" },
  { value: "camera_down", label: "Camera down" },
];

/** Theme of the tablet app. */
export const THEME_MODES: Option[] = [
  { value: "system", label: "System (Auto)" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** How a touchpad right click is made on the tablet. */
export const RIGHT_CLICK_METHODS: Option[] = [
  { value: "bottom_right_zone", label: "Bottom Right Zone" },
  { value: "two_finger_tap", label: "Two-Finger Tap" },
];

/** Which part of the tablet acts as the touchpad; touches outside the shape are ignored. */
export const TOUCHPAD_SHAPES: Option[] = [
  { value: "full_screen", label: "Full Screen" },
  { value: "standard_rectangle", label: "Standard Rectangle" },
  { value: "wide_rectangle", label: "Wide Rectangle" },
];
