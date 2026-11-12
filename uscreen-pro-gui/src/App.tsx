/*
 * Root of the UScreen Pro window.
 * Owns the shared state (daemon status, config and its saved fingerprints), the three-way merge of tablet-side changes into the open window, and routing between the views.
 * Does not own how a view looks (src/views, src/components) or how settings are stored (src-tauri).
 */
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
import type { AppConfig, CurvePreset, DaemonStatus, ForgetReport, PipelineEvent, TabletProfile, View } from "./lib/types";
import { DeviceSettings } from "./views/DeviceSettings";
import { Advanced } from "./views/Advanced";
import { GlobalSettings } from "./views/GlobalSettings";
import { Health } from "./views/Health";
import "./index.css";

const SCROLL_PREFIX = "uscreen-scroll:";

/**
 * The remembered scroll offset in pixels of the view [key] for this session, or 0 when none is stored or storage is unavailable.
 */
function readScroll(key: string): number {
  try {
    return Number(sessionStorage.getItem(SCROLL_PREFIX + key)) || 0;
  } catch {
    return 0;
  }
}

/**
 * Remembers the scroll offset [top] of the view [key] for the session; silently skipped when storage is unavailable.
 */
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

/**
 * Three-way merge of the settings file as it is now (`remote`) into the window's copy (`local`),
 * given what the window last loaded (`base`): a value the user has not touched here takes the new
 * file value (so changes made on the tablet appear), a value they edited here stays theirs.
 */
function mergeConfig(base: AppConfig, local: AppConfig, remote: AppConfig): AppConfig {
  /**
   * The merged value: the local one if it differs from the base (edited here), otherwise the one now in the file.
   */
  const take = (b: unknown, l: unknown, r: unknown) => (stable(l) !== stable(b) ? l : r);
  const merged: Record<string, unknown> = {};
  const keys = new Set([...Object.keys(remote), ...Object.keys(local)]);
  for (const k of keys) {
    if (k === "tablets") continue;
    /** Reads the top-level setting [k] from an object. */
    const rec = (o: unknown) => (o as Record<string, unknown>)[k];
    merged[k] = take(rec(base), rec(local), rec(remote));
  }
  const tablets: Record<string, TabletProfile> = {};
  for (const serial of new Set([...Object.keys(remote.tablets), ...Object.keys(local.tablets)])) {
    const r = remote.tablets[serial];
    const l = local.tablets[serial];
    const b = base.tablets[serial];
    if (!r) {
      // Gone from the file: keep it only if there are unsaved edits to it here.
      if (l && b && stable(l) !== stable(b)) tablets[serial] = l;
      continue;
    }
    if (!l || !b) {
      tablets[serial] = r;
      continue;
    }
    const profile: Record<string, unknown> = {};
    for (const k of new Set([...Object.keys(r), ...Object.keys(l)])) {
      /** Reads the profile setting [k] from an object. */
      const rec = (o: unknown) => (o as Record<string, unknown>)[k];
      const v = take(rec(b), rec(l), rec(r));
      if (v !== undefined) profile[k] = v;
    }
    tablets[serial] = profile as unknown as TabletProfile;
  }
  return { ...(merged as unknown as AppConfig), tablets };
}

/** A readable message from a thrown value. */
const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * Settings whose change restarts the video stream or the daemon. Every other setting saves by itself a moment after it
 * is changed; these wait for an explicit Apply so the stream does not restart while you are still choosing.
 */
const HEAVY = new Set([
  "encoder", "fps", "bitrate", "quality", "stream_scale", "width", "height", "position", "ten_bit", "vaapi_device",
  "pointer_handoff", "auto_resolution", "video_port", "input_port", "max_tablets", "max_performance_mode",
  "hevc_low_latency_tuning", "software_cursor", "keep_alive", "input_mode", "pen_only", "require_token", "wifi_address",
  "interface_size", "preferred_transport", "wireless_configured", "allow_legacy_fallback", "auto_enable_wifi_adb", "locked_apps",
]);

/** The top-level keys whose values differ between two objects. */
function changedKeys(a: object, b: object): string[] {
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  return [...new Set([...Object.keys(x), ...Object.keys(y)])].filter((k) => stable(x[k]) !== stable(y[k]));
}

/** Fingerprint of the global settings only; per-device edits have their own Save button. */
const globalsKey = (c: AppConfig) => JSON.stringify({ ...c, tablets: {} });

/**
 * Root of the UScreen Pro window.
 *
 * Owns all shared state: it polls the daemon status (every 1 s while a pipeline is restarting,
 * otherwise every 3 s), loads and saves `config.toml` through Tauri commands, tracks which device
 * pages have unsaved edits, and routes between Settings, Advanced, Health and per-device views.
 * Views below it are presentational and receive state and callbacks as props.
 */
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
  /** Opens the add-device wizard (with no preselected device) or closes it. */
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

  /** Reads the daemon status once; a failure is only logged and the next poll tries again. */
  const fetchStatus = useCallback(async () => {
    try {
      setStatus(await invoke<DaemonStatus>("get_daemon_status"));
    } catch (e) {
      console.error("get_daemon_status failed:", e);
    }
  }, []);

  /** What the settings file held when this window last read it; edits are measured against it. */
  const baseRef = useRef<AppConfig | null>(null);
  const configRef = useRef<AppConfig | null>(null);

  /**
   * Loads `config.toml` through Tauri and resets everything that measures edits against it: the base copy, the saved fingerprints of the globals and of each device profile.
   */
  const fetchConfig = useCallback(async () => {
    try {
      const cfg = await invoke<AppConfig>("get_config");
      baseRef.current = cfg;
      setConfig(cfg);
      setSavedConfig(globalsKey(cfg));
      setSavedProfiles(Object.fromEntries(Object.entries(cfg.tablets).map(([k, v]) => [k, stable(v)])));
    } catch (e) {
      toast.error(`Could not load config: ${errText(e)}`);
    }
  }, []);

  /*
   * The tablet app writes its own changes into the settings file, so the file is re-read every 1.5 s
   * and merged in: what changed there shows here at once, without losing what is being
   * edited here and not yet saved.
   */
  const syncConfig = useCallback(async () => {
    const base = baseRef.current;
    const local = configRef.current;
    if (!base || !local) return;
    try {
      const remote = await invoke<AppConfig>("get_config");
      if (stable(remote) === stable(base)) return;
      const merged = mergeConfig(base, local, remote);
      baseRef.current = remote;
      setConfig(merged);
      setSavedConfig(globalsKey(remote));
      setSavedProfiles(Object.fromEntries(Object.entries(remote.tablets).map(([k, v]) => [k, stable(v)])));
    } catch {
      /* a failed read just means the next poll tries again */
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
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    const timer = setInterval(() => void syncConfig(), 1500);
    return () => clearInterval(timer);
  }, [syncConfig]);

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
    /**
     * Keyboard shortcuts: Ctrl or Alt plus N opens the wizard, 1/2/3 switch to Settings, Advanced and Health.
     */
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

  /** Edits global settings in the window's copy only; saving is separate. */
  const patchConfig = (patch: Partial<AppConfig>) => setConfig((c) => (c ? { ...c, ...patch } : c));

  /** Keys of the global settings edited here and not yet saved. */
  const globalPending = useMemo(() => {
    if (!config || !savedConfig) return [] as string[];
    try {
      return changedKeys(JSON.parse(savedConfig), { ...config, tablets: {} });
    } catch {
      return [] as string[];
    }
  }, [config, savedConfig]);
  /** Edits that need an explicit Apply (they restart the stream). */
  const globalNeedsApply = globalPending.some((k) => HEAVY.has(k));
  /** "saving" while an autosave is running, "saved" for a few seconds after. */
  const [saveNote, setSaveNote] = useState<"" | "saving" | "saved">("");

  /** Tells the user a change was saved by itself, with a way back. */
  const announceSaved = (undo: () => Promise<void>) => {
    setSaveNote("saved");
    window.setTimeout(() => setSaveNote((n) => (n === "saved" ? "" : n)), 3000);
    toast.action("Saved", {
      id: "autosave",
      duration: 5000,
      action: { label: "Undo", onClick: () => void undo() },
    });
  };

  /** Writes the global settings to the file, then re-syncs and confirms with a toast. */
  const saveConfig = () =>
    guarded(async () => {
      if (!config) return;
      await invoke("save_config", { config });
      await syncConfig();
      toast.success("Settings saved");
    }, "Save failed");

  /*
   * Saved fade curves belong to the global config but are used from every device page, which saves only
   * its own profile. So they are written the moment they change, and the "unsaved" fingerprint of the
   * globals is updated for just that key, leaving other pending edits pending.
   */
  const savePresets = async (curve_presets: CurvePreset[]) => {
    try {
      await invoke("save_curve_presets", { presets: curve_presets });
      patchConfig({ curve_presets });
      setSavedConfig((prev) => {
        try {
          return JSON.stringify({ ...JSON.parse(prev), curve_presets });
        } catch {
          return prev;
        }
      });
    } catch (e) {
      toast.error(`Could not save presets: ${errText(e)}`);
    }
  };

  /** Turns the login autostart on or off and shows the state that was actually set. */
  const toggleAutostart = async (enabled: boolean) => {
    try {
      setAutostart(await invoke<boolean>("set_autostart_status", { enabled }));
    } catch (e) {
      toast.error(`Autostart: ${errText(e)}`);
    }
  };

  /* Autosave of the global settings: a second after the last light edit; heavy edits wait for Apply. */
  useEffect(() => {
    if (view !== "settings" || !config || !dirty || globalNeedsApply || busy) return;
    const timer = window.setTimeout(async () => {
      const before = baseRef.current;
      const now = configRef.current;
      if (!now) return;
      setSaveNote("saving");
      try {
        await invoke("save_config", { config: now });
        await syncConfig();
        announceSaved(async () => {
          if (!before) return;
          await invoke("save_config", { config: before });
          await fetchConfig();
          setSaveNote("");
        });
      } catch (e) {
        setSaveNote("");
        toast.error(`Autosave failed: ${errText(e)}`);
      }
    }, 900);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config, dirty, globalNeedsApply, view, busy]);

  const serial = typeof view === "object" ? view.device : null;
  const device = status?.devices.find((d) => d.serial === serial);
  const profile: TabletProfile | null =
    serial && config ? (config.tablets[serial] ?? { label: device?.model ?? serial }) : null;

  const profileDirty =
    !!serial && !!profile && stable(profile) !== (savedProfiles[serial] ?? stable({ label: device?.model ?? serial }));

  /**
   * Edits the open device's profile in the window's copy (creating it from the model name if absent); saving is separate.
   */
  const patchProfile = (patch: Partial<TabletProfile>) => {
    if (!serial) return;
    setConfig((c) =>
      c ? { ...c, tablets: { ...c.tablets, [serial]: { ...(c.tablets[serial] ?? { label: device?.model ?? serial }), ...patch } } } : c,
    );
  };

  /** Keys of the open device's profile edited here and not yet saved. */
  const profilePending = useMemo(() => {
    if (!serial || !profile) return [] as string[];
    try {
      return changedKeys(JSON.parse(savedProfiles[serial] ?? stable({ label: device?.model ?? serial })), profile);
    } catch {
      return [] as string[];
    }
  }, [serial, profile, savedProfiles, device?.model]);
  const profileNeedsApply = profilePending.some((k) => HEAVY.has(k));

  /* Autosave of the open device's profile, with the same rule as the global settings. */
  useEffect(() => {
    if (!serial || !profile || !profileDirty || profileNeedsApply || busy) return;
    const timer = window.setTimeout(async () => {
      const before = savedProfiles[serial];
      setSaveNote("saving");
      try {
        await invoke("save_device_profile", { serial, profile });
        setSavedProfiles((m) => ({ ...m, [serial]: stable(profile) }));
        await syncConfig();
        announceSaved(async () => {
          if (!before) return;
          await invoke("save_device_profile", { serial, profile: JSON.parse(before) });
          await fetchConfig();
          setSaveNote("");
        });
      } catch (e) {
        setSaveNote("");
        toast.error(`Autosave failed: ${errText(e)}`);
      }
    }, 900);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, profile, profileDirty, profileNeedsApply, busy]);

  /** Writes the open device's profile to the file and marks it as saved. */
  const saveProfile = () =>
    guarded(async () => {
      if (!serial || !profile) return;
      await invoke("save_device_profile", { serial, profile });
      setSavedProfiles((m) => ({ ...m, [serial]: stable(profile) }));
      await syncConfig();
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

  /** Pauses or resumes the open device's session. */
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

  /**
   * Starts or stops the daemon, showing the starting or stopping state until the status is read again.
   */
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

  /** Restarts the adb server, then re-reads the status. */
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
            wirelessSerials={Object.entries(config?.tablets ?? {}).filter(([, t]) => t.wifi_address).map(([k]) => k)}
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
                    dirty={dirty && globalNeedsApply}
                    saveNote={saveNote}
                    autostart={autostart}
                    onChange={patchConfig}
                    onSave={saveConfig}
                    onAutostart={toggleAutostart}
                    onPresetsChange={savePresets}
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
                    onPresetsChange={savePresets}
                    onSave={saveProfile}
                    onDelete={deleteProfile}
                    saveNote={saveNote}
                    onTogglePause={togglePause}
                    onInstallApk={() => queueInstall(serial)}
                    onConfigureWireless={() => setWizard({ serial, reconfigure: true })}
                    dirty={profileDirty && profileNeedsApply}
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
