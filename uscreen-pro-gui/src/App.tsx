import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { AddDeviceWizard } from "./components/AddDeviceWizard";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { TooltipProvider } from "./components/ui/layout";
import { Notifications, toast } from "./lib/toast";
import type { AppConfig, DaemonStatus, TabletProfile, View } from "./lib/types";
import { DeviceSettings } from "./views/DeviceSettings";
import { Advanced } from "./views/Advanced";
import { GlobalSettings } from "./views/GlobalSettings";
import { Health } from "./views/Health";
import "./index.css";

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Fingerprint of the global settings only; per-device edits have their own Save button. */
const globalsKey = (c: AppConfig) => JSON.stringify({ ...c, tablets: {} });

export default function App() {
  const [view, setView] = useState<View>("settings");
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [savedConfig, setSavedConfig] = useState("");
  const [autostart, setAutostart] = useState(false);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [daemonBusy, setDaemonBusy] = useState<"starting" | "stopping" | null>(null);
  // The library's baseline scheme is keyed on the `.dark` class (see index.css); index.html sets it pre-paint.
  const [dark, setDark] = useState(() => document.documentElement.classList.contains("dark"));

  const toggleTheme = () => {
    const next = !dark;
    setDark(next);
    document.documentElement.classList.toggle("dark", next);
    try {
      localStorage.setItem("uscreen-theme", next ? "dark" : "light");
    } catch {
      /* storage unavailable: the choice just won't persist */
    }
  };

  const fetchStatus = useCallback(async () => {
    try {
      setStatus(await invoke<DaemonStatus>("get_daemon_status"));
    } catch (e) {
      console.error("get_daemon_status failed:", e);
    }
  }, []);

  const fetchConfig = useCallback(async () => {
    try {
      const cfg = await invoke<AppConfig>("get_config");
      setConfig(cfg);
      setSavedConfig(globalsKey(cfg));
    } catch (e) {
      toast.error(`Could not load config: ${errText(e)}`);
    }
  }, []);

  useEffect(() => {
    void fetchConfig();
    void fetchStatus();
    invoke<boolean>("get_autostart_status").then(setAutostart).catch(() => {});
    const timer = setInterval(fetchStatus, 3000);
    return () => clearInterval(timer);
  }, [fetchConfig, fetchStatus]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.altKey)) return;
      if (e.key === "n") setWizardOpen(true);
      else if (e.key === "1") setView("settings");
      else if (e.key === "2") setView("advanced");
      else if (e.key === "3") setView("health");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const dirty = useMemo(() => config !== null && globalsKey(config) !== savedConfig, [config, savedConfig]);

  /** Run an async action behind the shared busy spinner, reporting failures as a toast. */
  async function guarded(action: () => Promise<void>, failure: string) {
    setBusy(true);
    try {
      await action();
    } catch (e) {
      toast.error(`${failure}: ${errText(e)}`);
    } finally {
      setBusy(false);
    }
  }

  const patchConfig = (patch: Partial<AppConfig>) => setConfig((c) => (c ? { ...c, ...patch } : c));

  const saveConfig = () =>
    guarded(async () => {
      if (!config) return;
      await invoke("save_config", { config });
      setSavedConfig(globalsKey(config));
      toast.success("Settings saved");
    }, "Save failed");

  const toggleAutostart = async (enabled: boolean) => {
    try {
      setAutostart(await invoke<boolean>("set_autostart_status", { enabled }));
    } catch (e) {
      toast.error(`Autostart: ${errText(e)}`);
    }
  };

  const serial = typeof view === "object" ? view.device : null;
  const device = status?.devices.find((d) => d.serial === serial);
  const profile: TabletProfile | null =
    serial && config ? (config.tablets[serial] ?? { label: device?.model ?? serial }) : null;

  const patchProfile = (patch: Partial<TabletProfile>) => {
    if (!serial) return;
    setConfig((c) =>
      c ? { ...c, tablets: { ...c.tablets, [serial]: { ...(c.tablets[serial] ?? { label: device?.model ?? serial }), ...patch } } } : c,
    );
  };

  const saveProfile = () =>
    guarded(async () => {
      if (!serial || !profile) return;
      await invoke("save_device_profile", { serial, profile });
      toast.success(`Saved ${profile.nickname || serial}`);
    }, "Save failed");

  const deleteProfile = () =>
    guarded(async () => {
      if (!serial) return;
      await invoke("delete_device_profile", { serial });
      await fetchConfig();
      setView("settings");
      toast.info("Device forgotten");
    }, "Delete failed");

  const togglePause = () =>
    guarded(async () => {
      if (!device) return;
      await invoke(device.paused ? "resume_device" : "pause_device", { serial: device.serial });
      await fetchStatus();
    }, "Control failed");

  const installApk = () =>
    guarded(async () => {
      if (!serial) return;
      toast.loading("Installing app", { id: "apk" });
      try {
        await invoke("install_apk_on_device", { serial });
        toast.success("App installed", { id: "apk" });
      } catch (e) {
        toast.error(errText(e), { id: "apk" });
      }
      await fetchStatus();
    }, "Install failed");

  const toggleDaemon = async () => {
    const running = status?.is_running ?? false;
    setDaemonBusy(running ? "stopping" : "starting");
    try {
      toast.success(await invoke<string>(running ? "stop_daemon" : "launch_daemon"));
    } catch (e) {
      toast.error(`${running ? "Stop" : "Start"} failed: ${errText(e)}`);
    } finally {
      setDaemonBusy(null);
      await fetchStatus();
    }
  };

  const restartAdb = () =>
    guarded(async () => {
      await invoke("restart_adb_server");
      toast.success("ADB server restarted");
      await fetchStatus();
    }, "ADB restart failed");

  return (
    <TooltipProvider>
      <div className="flex h-full w-full flex-col overflow-hidden bg-background text-foreground">
        <Notifications dark={dark} />
        <AddDeviceWizard
          open={wizardOpen}
          onOpenChange={setWizardOpen}
          hostVersion={status?.host_version}
          onDeviceAdded={async (added) => {
            await Promise.all([fetchStatus(), fetchConfig()]);
            setView({ device: added });
          }}
        />

        <TitleBar dark={dark} onToggleTheme={toggleTheme} />

        <div className="flex min-h-0 flex-1">
          <Sidebar
            view={view}
            devices={status?.devices ?? []}
            config={config}
            daemonRunning={status?.is_running ?? false}
            daemonPid={status?.pid}
            daemonBusy={daemonBusy}
            onToggleDaemon={toggleDaemon}
            onSelect={setView}
            onAddDevice={() => setWizardOpen(true)}
          />

          <main className="flex min-w-0 flex-1 flex-col bg-background">
            <div className="flex-1 overflow-y-auto p-4">
              <div className="mx-auto w-full max-w-5xl">
                {view === "settings" && config && (
                  <GlobalSettings
                    config={config}
                    dirty={dirty}
                    autostart={autostart}
                    onChange={patchConfig}
                    onSave={saveConfig}
                    onAutostart={toggleAutostart}
                  />
                )}
                {view === "advanced" && config && (
                  <Advanced config={config} dirty={dirty} onChange={patchConfig} onSave={saveConfig} />
                )}
                {view === "health" && status && <Health status={status} onRestartAdb={restartAdb} />}
                {serial && profile && config && (
                  <DeviceSettings
                    key={serial}
                    serial={serial}
                    device={device}
                    profile={profile}
                    config={config}
                    hostVersion={status?.host_version}
                    onChange={patchProfile}
                    onSave={saveProfile}
                    onDelete={deleteProfile}
                    onTogglePause={togglePause}
                    onInstallApk={installApk}
                  />
                )}
              </div>
            </div>

            {/* The daemon state lives in the sidebar card; the status bar only carries build info. */}
            <footer className="flex shrink-0 items-center justify-between px-4 pb-4 pt-2 text-xs text-muted-foreground">
              <span className="flex items-center gap-2">
                UScreen Pro
                {busy && <RefreshCw className="size-3 animate-spin" />}
              </span>
              <span className="flex items-center gap-2 font-mono">
                {status?.host_version ?? "-"}
                {status?.is_dirty && (
                  <Badge variant="secondary">dirty</Badge>
                )}
              </span>
            </footer>
          </main>
        </div>
      </div>
    </TooltipProvider>
  );
}
