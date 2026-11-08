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
