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
  software_cursor: false,
  keep_alive: false,
  locked_apps: ["com.android.chrome"],
  allow_legacy_fallback: false,
  auto_enable_wifi_adb: true,
  input_mode: "display",
  show_stats: false,
  touch_input: true,
  touch_on_lock_screen: false,
  keep_display_alive_when_locked: true,
  keep_connected_in_background: false,
  blank_pen_only: true,
  orientation: "auto",
  theme_mode: "system",
  battery_saver_lock: false,
  curve_presets: [],
  immersive_mode: false,
  battery_saver_on_disconnect: false,
  idle_dim_enabled: false,
  idle_dim_timeout_secs: 60,
  dim_curve: "ease_out",
  undim_curve: "ease_in",
  dim_duration_ms: 3000,
  undim_duration_ms: 500,
  touchpad_haptics: true,
  native_gestures: true,
  touchpad_sensitivity: 100,
  right_click_method: "bottom_right_zone",
  touchpad_shape: "full_screen",
  stylus_style: "mouse",
  stylus_button_primary: "right",
  stylus_button_secondary: "middle",
  version: "dev",
  tablets: {},
};

/*
 * `?scenario=` picks the device set: `two` (default: a paired tablet and an unpaired phone, both on USB),
 * `multi` (a phone and a tablet, neither paired: the wizard must make the user pick), `empty` (only the unpaired phone, no paired devices) `wifi` (a single tablet that already has a Wi-Fi address) and `skipped` (a paired tablet whose wireless
 * setup was skipped). Every IPC call is recorded on `window.__ipcLog` so a test can assert which serial
 * each command was given.
 */
const scenario = new URLSearchParams(location.search).get("scenario") ?? "two";
const TABLET = { serial: "TABLET_SERIAL", model: "SM-X616B" };
const PHONE = { serial: "PHONE_SERIAL", model: "SM-A556E" };

if (scenario !== "empty" && scenario !== "multi") {
  config.tablets[TABLET.serial] = {
    label: TABLET.model,
    ...(scenario === "skipped" ? { wireless_configured: false } : {}),
    ...(scenario === "wifi" ? { wifi_address: "192.168.1.50:5555", wireless_configured: true } : {}),
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
  devices: scenario === "multi" ? [dev(PHONE, false), dev(TABLET, false)] : scenario === "empty" ? [dev(PHONE, false)] : [dev(TABLET, true), ...(scenario === "two" ? [dev(PHONE, false)] : [])],
});

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Install an in-browser fake of the Tauri IPC (including a log at `window.__ipcLog`) so the UI can be developed and screenshotted without the Rust backend. */
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
          /* `window.__unplugged = true` simulates pulling the USB cable (the wizard's disconnect banner). */
          if ((window as unknown as { __unplugged?: boolean }).__unplugged) return [];
          return status().devices.map((d) => ({ serial: d.serial, state: d.state, model: d.model, is_wifi: false }));
        case "identify_device":
          return "Notification sent and the device vibrated";
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
          /* `?bootstrap=blocked` reproduces a router that drops device-to-device traffic. */
          if (new URLSearchParams(location.search).get("bootstrap") === "blocked")
            throw "ROUTER_BLOCKED|192.168.1.50|This computer can see the device over USB but can't reach it at 192.168.1.50 over Wi-Fi.";
          return "192.168.1.50:5555";
        }
        case "detect_tailscale_ip":
          return "100.101.102.103";
        case "pair_via_tailscale": {
          await wait(700);
          const { targetIp } = args as { targetIp: string };
          if (targetIp.endsWith(".1")) throw "Could not connect via Tailscale. Please ensure: 1. Tailscale is running on both devices.\n\nDetails: failed to connect to " + targetIp + ":5555";
          return `${targetIp}:5555`;
        }
        case "save_curve_presets":
          config.curve_presets = (args as { presets: AppConfig["curve_presets"] }).presets;
          return null;
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
