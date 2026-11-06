export interface ConnectedDevice {
  serial: string;
  model: string;
  state: string;
  /** "USB", "Wi-Fi" or "Disconnected" */
  transport: string;
  paused: boolean;
  device_version?: string;
}

export interface AdbEntry {
  serial: string;
  state: string;
  model: string;
  is_wifi: boolean;
}

export interface DaemonStatus {
  is_running: boolean;
  pid?: number;
  control_connected: boolean;
  host_version: string;
  is_dirty: boolean;
  control_socket: string;
  devices: ConnectedDevice[];
}

export interface TabletProfile {
  label: string;
  nickname?: string;
  width?: number;
  height?: number;
  fps?: number;
  bitrate?: number;
  quality?: number;
  stream_scale?: number;
  position?: string;
  encoder?: string;
  ten_bit?: boolean;
  wifi_address?: string;
  keep_alive?: boolean;
  locked_apps?: string[];
  app_version?: string;
  preferred_transport?: string;
}

export interface AppConfig {
  encoder: string;
  fps: number;
  bitrate: number;
  width: number;
  height: number;
  quality: number;
  stream_scale: number;
  pen_only: boolean;
  position: string;
  ten_bit: boolean;
  require_token: boolean;
  check_updates: boolean;
  max_tablets: number;
  wifi_address: string;
  vaapi_device: string;
  pointer_handoff: boolean;
  auto_resolution: boolean;
  video_port: number;
  input_port: number;
  auto_launch_app: boolean;
  max_performance_mode: boolean;
  hevc_low_latency_tuning: boolean;
  keep_alive: boolean;
  locked_apps: string[];
  allow_legacy_fallback: boolean;
  auto_enable_wifi_adb: boolean;
  version: string;
  tablets: Record<string, TabletProfile>;
}

export type View = "settings" | "advanced" | "health" | { device: string };

export interface WirelessDeviceInfo {
  serial: string;
  model: string;
}

export interface PairingEvent {
  kind: "searching" | "pairing" | "success" | "error" | "timeout";
  message: string;
  connect_endpoint?: string | null;
}

export interface QrPairingSession {
  service_name: string;
  pairing_code: string;
  qr_payload: string;
}

export interface ManualPairResult {
  message: string;
  connect_endpoint?: string | null;
}
