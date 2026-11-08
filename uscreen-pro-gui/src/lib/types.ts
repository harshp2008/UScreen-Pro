/** A tablet as reported by the backend (`DaemonStatus.devices`); mirrors the Rust struct of the same name. */
export interface ConnectedDevice {
  serial: string;
  model: string;
  state: string;
  /** "USB", "Wi-Fi" or "Disconnected" */
  transport: string;
  paused: boolean;
  device_version?: string;
  /** Has a [tablets.<serial>] entry in config.toml; otherwise it is a "new device". */
  paired: boolean;
  /** This device's job in the daemon's APK install queue. */
  install?: InstallStatus | null;
}

/** State of a device's job in the daemon's APK install queue. */
export interface InstallStatus {
  state: "queued" | "installing" | "done" | "failed";
  detail: string;
}

/** Emitted by the backend while a device's ffmpeg/evdi_helper are respawned. */
export interface PipelineEvent {
  /** Device the event is about; null means every device. */
  serial: string | null;
  ok: boolean;
  live_ms?: number | null;
  message: string;
  /** Last event of one restart request. */
  done: boolean;
}

/** What forgetting a device actually removed; mirrors the Rust `ForgetReport`. */
export interface ForgetReport {
  removed_profiles: string[];
  session: string;
  disconnected: string[];
  pairing_records_removed: number;
  notes: string[];
}

/** One line of `adb devices -l`, before USB and Wi-Fi entries of the same tablet are merged. */
export interface AdbEntry {
  serial: string;
  state: string;
  model: string;
  is_wifi: boolean;
}

/** Snapshot of the host daemon polled by the UI. */
export interface DaemonStatus {
  is_running: boolean;
  pid?: number;
  control_connected: boolean;
  host_version: string;
  is_dirty: boolean;
  control_socket: string;
  devices: ConnectedDevice[];
}

/** Per-device settings; mirrors `[tablets.<id>]` in config.toml. Unset fields inherit the global value. */
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
  /** false when the user skipped wireless setup in the wizard; absent on older profiles means configured. */
  wireless_configured?: boolean;
}

/** Global settings plus all device profiles; mirrors config.toml. */
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
  software_cursor: boolean;
  keep_alive: boolean;
  locked_apps: string[];
  allow_legacy_fallback: boolean;
  auto_enable_wifi_adb: boolean;
  version: string;
  tablets: Record<string, TabletProfile>;
}

/** Which page is showing: a named page, or a device's own settings page. */
export type View = "settings" | "advanced" | "health" | { device: string };

/** Identity of a device reached over Wi-Fi. */
export interface WirelessDeviceInfo {
  serial: string;
  model: string;
}

/** Progress of an in-flight wireless pairing attempt. */
export interface PairingEvent {
  kind: "searching" | "pairing" | "success" | "error" | "timeout";
  message: string;
  connect_endpoint?: string | null;
}

/** Data for rendering the pairing QR code. */
export interface QrPairingSession {
  service_name: string;
  pairing_code: string;
  qr_payload: string;
}

/** Result of pairing with a typed address and code. */
export interface ManualPairResult {
  message: string;
  connect_endpoint?: string | null;
}

/** `install_progress` / `pair_progress`: one human-readable step of the wizard's USB work. */
export interface ProgressEvent {
  serial?: string | null;
  message: string;
}
