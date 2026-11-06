/**
 * Browser-only IPC stub so `npm run dev` renders the full UI without the Tauri shell (design review,
 * screenshots, hover/state-layer inspection). Loaded by main.tsx only when `import.meta.env.DEV`
 * is true and no Tauri runtime is present; it is tree-shaken out of release builds.
 */
import { emit } from "@tauri-apps/api/event";
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

/*
 * `?scenario=` picks the device set: `two` (default: a paired tablet and an unpaired phone, both on USB),
 * `empty` (only the unpaired phone, no paired devices) and `skipped` (a paired tablet whose wireless
 * setup was skipped). Every IPC call is recorded on `window.__ipcLog` so a test can assert which serial
 * each command was given.
 */
const scenario = new URLSearchParams(location.search).get("scenario") ?? "two";
const TABLET = { serial: "TABLET_SERIAL", model: "SM-X616B" };
const PHONE = { serial: "PHONE_SERIAL", model: "SM-A556E" };

if (scenario !== "empty") {
  config.tablets[TABLET.serial] = {
    label: TABLET.model,
    ...(scenario === "skipped" ? { wireless_configured: false } : {}),
  };
}

const dev = (d: { serial: string; model: string }, paired: boolean) => ({
  ...d,
  state: "device",
  transport: "USB",
  paused: false,
  device_version: "dev-mock",
  paired,
  install: null,
});

const status = (): DaemonStatus => ({
  is_running: running,
  pid: running ? 4242 : undefined,
  control_connected: running,
  host_version: "dev-mock",
  is_dirty: false,
  control_socket: "/run/user/1000/uscreen.sock",
  devices: scenario === "empty" ? [dev(PHONE, false)] : [dev(TABLET, true), ...(scenario === "two" ? [dev(PHONE, false)] : [])],
});

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export function installDevMock() {
  const log: { cmd: string; args: unknown }[] = [];
  (window as unknown as { __ipcLog: typeof log }).__ipcLog = log;
  mockIPC(
    async (cmd, args) => {
      log.push({ cmd, args });
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
        case "list_adb_devices":
          return status().devices.map((d) => ({ serial: d.serial, state: d.state, model: d.model, is_wifi: false }));
        case "install_apk_on_device": {
          const serial = (args as { serial: string }).serial;
          await emit("install_progress", { serial, message: "Found old APK" });
          await wait(700);
          await emit("install_progress", { serial, message: "Removing old APK" });
          await wait(700);
          await emit("install_progress", { serial, message: "Installing new APK" });
          await wait(900);
          return "APK installed";
        }
        case "bootstrap_wireless_over_usb": {
          const serial = (args as { serial: string }).serial;
          await emit("pair_progress", { serial, message: "Starting ADB server" });
          await wait(700);
          await emit("pair_progress", { serial, message: "Pairing..." });
          await wait(700);
          await emit("pair_progress", { serial, message: "Connecting..." });
          await wait(700);
          return "192.168.1.50:5555";
        }
        case "save_device_profile": {
          const { serial, profile } = args as { serial: string; profile: AppConfig["tablets"][string] };
          config.tablets[serial] = profile;
          return null;
        }
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
