import { useTheme } from "./lib/theme";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { RefreshCw } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { AddDeviceWizard } from "./components/AddDeviceWizard";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { TooltipProvider } from "./components/ui/layout";
import { Notifications, toast } from "./lib/toast";
import type { AppConfig, DaemonStatus, ForgetReport, PipelineEvent, TabletProfile, View } from "./lib/types";
import { DeviceSettings } from "./views/DeviceSettings";
import { Advanced } from "./views/Advanced";
import { GlobalSettings } from "./views/GlobalSettings";
import { Health } from "./views/Health";
import "./index.css";

const SCROLL_PREFIX = "uscreen-scroll:";

function readScroll(key: string): number {
  try {
    return Number(sessionStorage.getItem(SCROLL_PREFIX + key)) || 0;
  } catch {
    return 0;
  }
}

function writeScroll(key: string, top: number) {
  try {
    sessionStorage.setItem(SCROLL_PREFIX + key, String(Math.round(top)));
  } catch {
    /* storage unavailable: scroll just won't be remembered */
  }
}

/** JSON with sorted keys, so re-adding a key later does not look like an edit. */
const stable = (v: unknown) =>
  JSON.stringify(v, (_k, x) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => a.localeCompare(b)))
      : x,
  );

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Fingerprint of the global settings only; per-device edits have their own Save button. */
const globalsKey = (c: AppConfig) => JSON.stringify({ ...c, tablets: {} });

export default function App() {
  const [view, setViewState] = useState<View>("settings");
  const scrollRef = useRef<HTMLDivElement>(null);
  const viewKey = typeof view === "object" ? `device:${view.device}` : view;
  const viewKeyRef = useRef(viewKey);
  const [status, setStatus] = useState<DaemonStatus | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [savedConfig, setSavedConfig] = useState("");
  /** Per-device fingerprints of what is on disk, to tell whether a device page has unsaved edits. */
  const [savedProfiles, setSavedProfiles] = useState<Record<string, string>>({});
  const [autostart, setAutostart] = useState(false);
  /* null = closed. `serial` pins the wizard to the clicked new device; `reconfigure` re-runs only wireless setup. */
  const [wizard, setWizard] = useState<{ serial?: string; reconfigure?: boolean } | null>(null);
  const setWizardOpen = (open: boolean) => setWizard(open ? {} : null);
  const [busy, setBusy] = useState(false);
  const [daemonBusy, setDaemonBusy] = useState<"starting" | "stopping" | null>(null);
  /** Devices whose ffmpeg/evdi_helper are being respawned; "*" means all of them. */
  const [restarting, setRestarting] = useState<ReadonlySet<string>>(new Set());
  const { pref: themePref, dark, setPref: setThemePref } = useTheme();

  /** Every tab change goes through here so the outgoing tab's scroll offset is saved first. */
  const setView = useCallback((next: View) => {
    if (scrollRef.current) writeScroll(viewKeyRef.current, scrollRef.current.scrollTop);
    setViewState(next);
  }, []);

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
      setSavedProfiles(Object.fromEntries(Object.entries(cfg.tablets).map(([k, v]) => [k, stable(v)])));
    } catch (e) {
      toast.error(`Could not load config: ${errText(e)}`);
    }
  }, []);

  /* Poll faster while something visible is in flight (an install or a restart). */
  const busyDevices = (status?.devices ?? []).some((d) => d.install?.state === "queued" || d.install?.state === "installing");
  const fastPoll = busyDevices || restarting.size > 0;

  useEffect(() => {
    void fetchConfig();
    void fetchStatus();
    invoke<boolean>("get_autostart_status").then(setAutostart).catch(() => {});
  }, [fetchConfig, fetchStatus]);

  useEffect(() => {
    const timer = setInterval(fetchStatus, fastPoll ? 1000 : 3000);
    return () => clearInterval(timer);
  }, [fetchStatus, fastPoll]);

  /*
   * The backend reports a settings-driven pipeline restart in two steps: `pipeline_restarting`
   * when ffmpeg/evdi_helper are about to be killed, `pipeline_restarted` once the new stream has
   * delivered a frame. The device card shows the spinner for exactly that window.
   */
  useEffect(() => {
    const subs = [
      listen<PipelineEvent>("pipeline_restarting", ({ payload }) => {
        setRestarting((s) => new Set(s).add(payload.serial ?? "*"));
      }),
      listen<PipelineEvent>("pipeline_restarted", ({ payload }) => {
        setRestarting((s) => {
          const next = new Set(s);
          if (payload.done) {
            if (payload.serial === null) next.clear();
            else next.delete(payload.serial);
            next.delete("*");
          } else if (payload.serial) next.delete(payload.serial);
          return next;
        });
        if (payload.done) {
          if (!payload.ok) toast.error(`Pipeline restart failed: ${payload.message}`);
        } else if (payload.ok) toast.success(`Stream restarted: ${payload.message}`);
        else toast.error(`Stream restart: ${payload.message}`);
        void fetchStatus();
      }),
    ];
    return () => {
      subs.forEach((p) => void p.then((fn) => fn()).catch(() => {}));
    };
  }, [fetchStatus]);

  /* Say how each queued install ended; the card itself only shows the live state. */
  const lastInstall = useRef<Record<string, string>>({});
  useEffect(() => {
    for (const d of status?.devices ?? []) {
      const now = d.install?.state ?? "";
      const before = lastInstall.current[d.serial] ?? "";
      if (now !== before && (now === "done" || now === "failed") && (before === "installing" || before === "queued")) {
        if (now === "done") toast.success(`${d.model}: ${d.install?.detail || "app updated"}`);
        else toast.error(`${d.model}: update failed. ${d.install?.detail ?? ""}`);
      }
      lastInstall.current[d.serial] = now;
    }
  }, [status]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.altKey)) return;
      if (e.key === "n") setWizard({});
      else if (e.key === "1") setView("settings");
      else if (e.key === "2") setView("advanced");
      else if (e.key === "3") setView("health");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /*
   * Restore the incoming tab's offset once its content is in the DOM. The config/status flags are
   * dependencies because the first render of a tab can happen before the data it shows has loaded.
   */
  const contentReady = config !== null && status !== null;
  useLayoutEffect(() => {
    viewKeyRef.current = viewKey;
    if (scrollRef.current) scrollRef.current.scrollTop = readScroll(viewKey);
  }, [viewKey, contentReady]);

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

  const profileDirty =
    !!serial && !!profile && stable(profile) !== (savedProfiles[serial] ?? stable({ label: device?.model ?? serial }));

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
      setSavedProfiles((m) => ({ ...m, [serial]: stable(profile) }));
      toast.success(`Saved ${profile.nickname || serial}`);
    }, "Save failed");

  /* Forget: remove the profile, kill the session, adb disconnect, then drop the card at once. */
  const deleteProfile = () =>
    guarded(async () => {
      if (!serial) return;
      const report = await invoke<ForgetReport>("forget_device", { serial });
      setStatus((s) => (s ? { ...s, devices: s.devices.filter((d) => d.serial !== serial) } : s));
      setView("settings");
      await Promise.all([fetchConfig(), fetchStatus()]);
      toast.info(
        report.disconnected.length > 0
          ? `Device forgotten. ${report.disconnected.length} adb disconnect(s) run`
          : "Device forgotten",
      );
    }, "Forget failed");

  const togglePause = () =>
    guarded(async () => {
      if (!device) return;
      await invoke(device.paused ? "resume_device" : "pause_device", { serial: device.serial });
      await fetchStatus();
    }, "Control failed");

  /* Both the Health buttons and the device page go through the daemon's install queue (two at a time). */
  const queueInstall = async (target: string | null = serial) => {
    if (!target) return;
    try {
      toast.info(`${await invoke<string>("queue_apk_install", { serial: target })}`);
    } catch (e) {
      toast.error(`Install failed on ${target}: ${errText(e)}`);
    }
    await fetchStatus();
  };

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
        {wizard && (
          <AddDeviceWizard
            open
            onOpenChange={setWizardOpen}
            hostVersion={status?.host_version}
            targetSerial={wizard.serial}
            pairedSerials={Object.keys(config?.tablets ?? {})}
            reconfigure={
              wizard.reconfigure && wizard.serial && config?.tablets[wizard.serial]
                ? { serial: wizard.serial, profile: config.tablets[wizard.serial] }
                : undefined
            }
            onDeviceAdded={async (added) => {
              await Promise.all([fetchStatus(), fetchConfig()]);
              setView({ device: added });
            }}
          />
        )}

        <TitleBar theme={themePref} onThemeChange={setThemePref} />

        <div className="flex min-h-0 flex-1">
          <Sidebar
            view={view}
            devices={status?.devices ?? []}
            config={config}
            restarting={restarting}
            daemonRunning={status?.is_running ?? false}
            daemonPid={status?.pid}
            daemonBusy={daemonBusy}
            onToggleDaemon={toggleDaemon}
            onSelect={setView}
            onAddDevice={(target) => setWizard({ serial: target })}
          />

          <main className="flex min-w-0 flex-1 flex-col bg-background">
            <div ref={scrollRef} className="flex-1 overflow-y-auto p-4">
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
                {view === "health" && status && <Health status={status} onRestartAdb={restartAdb} onInstallApk={queueInstall} />}
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
                    onInstallApk={() => queueInstall(serial)}
                    onConfigureWireless={() => setWizard({ serial, reconfigure: true })}
                    dirty={profileDirty}
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
