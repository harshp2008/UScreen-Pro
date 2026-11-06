/**
 * Browser-only IPC stub so `npm run dev` renders the full UI without the Tauri shell (design review,
 * screenshots, hover/state-layer inspection). Loaded by main.tsx only when `import.meta.env.DEV`
 * is true and no Tauri runtime is present; it is tree-shaken out of release builds.
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import type { AppConfig, DaemonStatus } from "./types";

let running = false;

const config: AppConfig = {
  encoder: "hevc_nvenc",
  fps: 90,
  bitrate: 30000,
  width: 0,
  height: 0,
  quality: 18,
  stream_scale: 1,
  pen_only: false,
  position: "right",
  ten_bit: false,
  require_token: false,
  check_updates: true,
  max_tablets: 2,
  wifi_address: "",
  vaapi_device: "/dev/dri/renderD128",
  pointer_handoff: true,
  auto_resolution: true,
  video_port: 5000,
  input_port: 5001,
  auto_launch_app: true,
  max_performance_mode: false,
  hevc_low_latency_tuning: true,
  keep_alive: false,
  locked_apps: ["com.android.chrome"],
  allow_legacy_fallback: false,
  auto_enable_wifi_adb: true,
  version: "dev",
  tablets: {},
};

const status = (): DaemonStatus => ({
  is_running: running,
  pid: running ? 4242 : undefined,
  control_connected: running,
  host_version: "dev-mock",
  is_dirty: false,
  control_socket: "/run/user/1000/uscreen.sock",
  devices: [
    { serial: "R52W1ABCDEF", model: "SM-X510", state: "device", transport: "USB", paused: false, device_version: "dev-mock" },
  ],
});

export function installDevMock() {
  mockIPC((cmd) => {
    switch (cmd) {
      case "get_daemon_status":
        return status();
      case "get_config":
        return config;
      case "get_autostart_status":
        return false;
      case "launch_daemon":
        running = true;
        return "Daemon started";
      case "stop_daemon":
        running = false;
        return "Daemon stopped";
      default:
        return null;
    }
  });
}
