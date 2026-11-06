export interface Option {
  value: string;
  label: string;
}

export const ENCODERS: Option[] = [
  { value: "h264_nvenc", label: "NVIDIA H.264 (NVENC)" },
  { value: "hevc_nvenc", label: "NVIDIA HEVC (NVENC)" },
  { value: "h264_vaapi", label: "Intel / AMD H.264 (VAAPI)" },
  { value: "hevc_vaapi", label: "Intel / AMD HEVC (VAAPI)" },
  { value: "libx264", label: "Software (libx264)" },
];

export const FPS: Option[] = [
  { value: "30", label: "30 fps" },
  { value: "60", label: "60 fps" },
  { value: "90", label: "90 fps" },
  { value: "120", label: "120 fps" },
];

export const POSITIONS: Option[] = [
  { value: "left", label: "Left" },
  { value: "right", label: "Right" },
  { value: "above", label: "Above" },
  { value: "below", label: "Below" },
];

export const STREAM_DETAIL: Option[] = [
  { value: "1", label: "Full" },
  { value: "2", label: "Half" },
];

export const TRANSPORTS: Option[] = [
  { value: "auto", label: "Auto (USB first, Wi-Fi fallback)" },
  { value: "usb", label: "USB only" },
  { value: "wifi", label: "Wi-Fi only" },
];

export function labelOf(options: Option[], value: string | number | undefined): string {
  return options.find((o) => o.value === String(value))?.label ?? String(value ?? "");
}

/** Encoder CRF/QP values behind the qualitative quality slider. 18 is the host default. */
export const QUALITY_STOPS: Option[] = [
  { value: "14", label: "Maximum sharpness" },
  { value: "18", label: "Balanced" },
  { value: "22", label: "Efficient" },
  { value: "26", label: "Low bandwidth" },
  { value: "30", label: "High compression" },
];

export const DEFAULT_QUALITY = 18;

/** Index of the stop closest to a stored CRF value (older configs may hold e.g. 17 or 12). */
export function nearestQualityIndex(quality: number): number {
  let best = 0;
  QUALITY_STOPS.forEach((s, i) => {
    if (Math.abs(Number(s.value) - quality) < Math.abs(Number(QUALITY_STOPS[best].value) - quality)) best = i;
  });
  return best;
}
