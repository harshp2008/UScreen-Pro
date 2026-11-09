/*
 * The 'Add device' dialog: Prepare Device, Connect USB, Install, Wireless, Finish, plus the no-cable wireless path and the 'reconfigure wireless' entry.
 * Owns the wizard's state and calls the Tauri adb and pairing commands; the generic building blocks are in wizard/WizardParts.tsx and the pairing helper dialog in ManualPairDialog.tsx.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Download,
  Network,
  RotateCcw,
  ScanSearch,
  ChevronLeft,
  ExternalLink,
  BadgeCheck,
  Siren,
  LoaderCircle,
  Smartphone,
  Usb,
  Wifi,
  X,
  Zap,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toast } from "../lib/toast";
import type {
  AdbEntry,
  PairingEvent,
  ProgressEvent,
  QrPairingSession,
  TabletProfile,
  WirelessDeviceInfo,
} from "../lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { TextInput } from "./ui/controls";
import { ManualPairDialog } from "./ManualPairDialog";
import { DebugTimeline, Instructions, Note, StepLog } from "./wizard/WizardParts";

/* Step 0 is the landing screen; steps 1-5 are the USB flow (the no-cable wireless path jumps to Finish). */
const STEPS = ["Prepare Device", "Connect USB", "Install", "Wireless", "Finish"] as const;
const WIRELESS_STEP = 4;
const FINISH_STEP = 5;
/** Wizard step: 0 landing, 1 Prepare Device, 2 Connect USB, 3 Install, 4 Wireless, 5 Finish. */
type Step = 0 | 1 | 2 | 3 | 4 | 5;
/** How the device is being added: with a cable first (`usb`) or the no-cable wireless path. */
type Via = "usb" | "wireless";
/** Progress of a background operation (installing the app, or the wireless setup). */
type InstallState = "idle" | "running" | "ok" | "failed";
/** The transport the device prefers once added: automatic, USB or Wi-Fi. */
type Transport = "auto" | "usb" | "wifi";
/** Which wireless method is shown: automatic pairing or the Tailscale address. */
type WirelessTab = "auto" | "tailscale";
/* Backend error prefix of a failed router reachability check (see pairing.rs). */
const ROUTER_BLOCKED_PREFIX = "ROUTER_BLOCKED|";
const TAILSCALE_DOWNLOAD_URL = "https://tailscale.com/download";

/** Props of the wizard. It is mounted only while open, so every field starts from its initial value. */
interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  hostVersion?: string;
  /** Pin the wizard to this device (the "New Devices" entry that was clicked). */
  targetSerial?: string;
  /** Serials that already have a profile; they are never offered as a new device. */
  pairedSerials?: string[];
  /** Serials whose saved profile already has a Wi-Fi address (wireless is set up for them). */
  wirelessSerials?: string[];
  /** Re-run only the Wireless step for an already paired device. */
  reconfigure?: { serial: string; profile: TabletProfile };
  onDeviceAdded: (serial: string) => void | Promise<void>;
}

/** A readable message from a thrown value. */
const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * Multi-step dialog that onboards a tablet: pick the device, install the APK over USB, then set up
 * wireless access (QR pairing, manual code, or Tailscale) or skip it. With `reconfigure` set it
 * opens directly on the wireless step for an already-paired device. Every adb call after the
 * device is chosen is pinned to its serial so a second plugged-in tablet cannot be touched.
 */
export function AddDeviceWizard({
  open,
  onOpenChange,
  hostVersion,
  targetSerial,
  pairedSerials = [],
  wirelessSerials = [],
  reconfigure,
  onDeviceAdded,
}: Props) {
  const [step, setStep] = useState<Step>(reconfigure ? WIRELESS_STEP : 0);
  const [via, setVia] = useState<Via>("usb");
  const [entries, setEntries] = useState<AdbEntry[]>([]);
  const [device, setDevice] = useState<AdbEntry | null>(null);

  const [install, setInstall] = useState<{ state: InstallState; message: string }>({ state: "idle", message: "" });

  const [wireless, setWireless] = useState<{ state: InstallState; message: string }>({ state: "idle", message: "" });
  /* True once the user chose "Skip for now (USB Only)": the device is saved with wireless_configured = false. */
  const [wirelessSkipped, setWirelessSkipped] = useState(false);
  const [installLog, setInstallLog] = useState<string[]>([]);
  const [pairLog, setPairLog] = useState<string[]>([]);
  /* The device picked on Connect USB; every later adb call is pinned to this serial. */
  const [chosenSerial, setChosenSerial] = useState<string | null>(null);

  const [qr, setQr] = useState<QrPairingSession | null>(null);
  const [pairEvent, setPairEvent] = useState<PairingEvent | null>(null);
  const [wirelessOpen, setWirelessOpen] = useState(false);
  /* Step 4 has two paths. Both keep their own state, so switching tabs never loses a result or typed address. */
  const [wirelessTab, setWirelessTab] = useState<WirelessTab>("auto");
  const [macOpen, setMacOpen] = useState(false);
  /* The always-available "Connection Failed?" help box of the Automatic tab. */
  const [failedHelpOpen, setFailedHelpOpen] = useState(false);
  const [tsHostIp, setTsHostIp] = useState("");
  const [tsTargetIp, setTsTargetIp] = useState("");
  const [ts, setTs] = useState<{ state: InstallState; message: string; detail?: string }>({ state: "idle", message: "" });
  const tsDetectTried = useRef(false);
  const [manualOpen, setManualOpen] = useState(false);
  /* The device the user explicitly chose on Connect USB ("Setup this device"); never guessed. */
  const [pickedSerial, setPickedSerial] = useState<string | null>(null);
  const [identifying, setIdentifying] = useState<string | null>(null);
  const [qrNonce, setQrNonce] = useState(0);
  /* True once discovery found nothing for 30 s: the router drops multicast, so QR pairing cannot work. */
  const [mdnsBlocked, setMdnsBlocked] = useState(false);
  const [pairError, setPairError] = useState<string | null>(null);

  const [wifiEndpoint, setWifiEndpoint] = useState<string | null>(null);
  const [transport, setTransport] = useState<Transport>("auto");
  const [nickname, setNickname] = useState("");
  const [saving, setSaving] = useState(false);

  /*
   * Which USB device this run works on. A clicked "New Devices" entry or a reconfigure pins it; otherwise
   * it is an unpaired USB device (never a paired one), and once the user moves on it is frozen so a
   * second plugged-in device can never take over a running install.
   */
  const pinned = reconfigure?.serial ?? targetSerial ?? null;
  const usbEntries = entries.filter((e) => !e.is_wifi);
  const candidates = pinned
    ? usbEntries.filter((e) => e.serial === pinned)
    : usbEntries.filter((e) => !pairedSerials.includes(e.serial));
  /*
   * With more than one USB device attached nothing is chosen for the user: `usb` stays null until they
   * press "Setup this device" on one (or the wizard was pinned to a device). That click is the only way a
   * serial gets into `pickedSerial`, and every later adb call is pinned to it.
   */
  const needsPick = !pinned && usbEntries.length > 1;
  const lockedSerial = pinned ?? pickedSerial ?? chosenSerial;
  const usb = lockedSerial
    ? (usbEntries.find((e) => e.serial === lockedSerial) ?? null)
    : needsPick
      ? null
      : (candidates[0] ?? null);
  /* The device the landing screen's guide is about: pinned, picked, or the only one plugged in. */
  const contextSerial = pinned ?? pickedSerial ?? (usbEntries.length === 1 ? usbEntries[0].serial : null);
  const installStarted = useRef(false);
  /* Bumped when the cable drops so a result that arrives from an interrupted run is ignored. */
  const installRun = useRef(0);
  const wirelessRun = useRef(0);
  /* USB: has an authorized device been seen on step 2+ (so its absence now means "unplugged"), and has it stayed gone. */
  const [usbSeen, setUsbSeen] = useState(false);
  const [usbGone, setUsbGone] = useState(false);
  const [retryingConn, setRetryingConn] = useState(false);
  /* A successful USB bootstrap makes the mDNS check pointless, so a late timeout event is ignored. */
  const usbBootstrapped = useRef(false);
  const stepRef = useRef<Step>(0);
  stepRef.current = step;
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [moreBelow, setMoreBelow] = useState(false);

  /**
   * Lists adb devices once; keeps the previous array when nothing changed so the 1.5 s poll does not re-render the dialog.
   */
  const poll = useCallback(async () => {
    try {
      const next = (await invoke<AdbEntry[] | null>("list_adb_devices")) ?? [];
      // Keep the old array when nothing changed so the 1.5 s poll does not re-render the dialog.
      setEntries((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
    } catch (e) {
      console.error("list_adb_devices failed:", e);
    }
  }, []);

  /*
   * The wizard is mounted only while open (see App), so every field starts from its initial value
   * on the first render. Resetting in an effect instead would paint the previous run's step first.
   */
  useEffect(() => {
    void poll();
    const timer = setInterval(poll, 1500);
    return () => clearInterval(timer);
  }, [poll]);

  useEffect(() => {
    if (usb && via === "usb") setDevice(usb);
  }, [usb, via]);

  /**
   * Installs the app on [serial] over USB and records the outcome; a result from a superseded run is ignored.
   */
  const runInstall = useCallback(async (serial: string) => {
    const run = ++installRun.current;
    setInstallLog([]);
    setInstall({ state: "running", message: "Installing over USB" });
    try {
      await invoke("install_apk_on_device", { serial });
      if (run === installRun.current) setInstall({ state: "ok", message: "App installed" });
    } catch (e) {
      if (run === installRun.current) setInstall({ state: "failed", message: errText(e) });
    }
  }, []);

  /* Reconfigure mode saves straight into the existing profile and closes; returns an error text on failure. */
  const persistReconfigure = useCallback(
    async (serial: string, endpoint: string): Promise<string | null> => {
      if (!reconfigure) return null;
      setSaving(true);
      try {
        await invoke("save_device_profile", {
          serial,
          profile: { ...reconfigure.profile, wifi_address: endpoint, wireless_configured: true },
        });
        toast.success("Wireless mode configured");
        await onDeviceAdded(serial);
        onOpenChange(false);
        return null;
      } catch (e) {
        return errText(e);
      } finally {
        setSaving(false);
      }
    },
    [reconfigure, onDeviceAdded, onOpenChange],
  );

  /* Once installed, switch the device to wireless mode over the same cable. */
  const runBootstrap = useCallback(async (serial: string) => {
    const run = ++wirelessRun.current;
    setPairLog([]);
    setWireless({ state: "running", message: "Configuring wireless mode over USB..." });
    try {
      const endpoint = await invoke<string>("bootstrap_wireless_over_usb", { serial });
      if (run !== wirelessRun.current) return;
      usbBootstrapped.current = true;
      void invoke("cancel_qr_pairing");
      setWifiEndpoint(endpoint);
      setWireless({ state: "ok", message: `Wireless mode ready at ${endpoint}` });
      /* Outside reconfigure there is no auto-advance: the green tick stays until the user presses Next. */
      const failure = await persistReconfigure(serial, endpoint);
      if (failure) setWireless({ state: "failed", message: `Wireless is on, but saving failed: ${failure}` });
    } catch (e) {
      if (run === wirelessRun.current) setWireless({ state: "failed", message: errText(e) });
    }
  }, [persistReconfigure]);

  /* Tailscale path: adb over the tunnel on port 5555; the backend remembers both addresses. */
  async function runTailscale() {
    setTs({ state: "running", message: "Connecting through Tailscale..." });
    try {
      const endpoint = await invoke<string>("pair_via_tailscale", {
        hostIp: tsHostIp.trim(),
        targetIp: tsTargetIp.trim(),
        serial: usb?.state === "device" ? usb.serial : null,
      });
      usbBootstrapped.current = true;
      void invoke("cancel_qr_pairing");
      setWifiEndpoint(endpoint);
      setTs({ state: "ok", message: endpoint });
      const failure = await persistReconfigure(reconfigure?.serial ?? "", endpoint);
      if (failure) setTs({ state: "failed", message: `Connected, but saving failed: ${failure}` });
    } catch (e) {
      const text = errText(e);
      const at = text.indexOf("\n\nDetails:");
      setTs(
        text.startsWith("Could not connect via Tailscale")
          ? { state: "failed", message: "tailscale", detail: at >= 0 ? text.slice(at + 10).trim() : undefined }
          : { state: "failed", message: text },
      );
    }
  }

  /** Fills this PC's Tailscale address; a failure is shown only when [quiet] is false. */
  async function detectTailscaleIp(quiet: boolean) {
    try {
      setTsHostIp(await invoke<string>("detect_tailscale_ip"));
    } catch (e) {
      if (!quiet) toast.error(errText(e));
    }
  }

  /* Fill "this PC" the first time the Tailscale tab is shown. */
  useEffect(() => {
    if (step !== WIRELESS_STEP || wirelessTab !== "tailscale" || tsDetectTried.current) return;
    tsDetectTried.current = true;
    void detectTailscaleIp(true);
  }, [step, wirelessTab]);

  /*
   * Unplug detection. Once an authorized device has been seen on Connect USB / Install / Wireless, its
   * absence halts the wizard. The delay rides out the brief adb reconnect after `adb tcpip` restarts adbd.
   */
  const usbOnline = usb?.state === "device";
  useEffect(() => {
    if (step < 2) setUsbSeen(false);
    else if (usbOnline && via === "usb") setUsbSeen(true);
  }, [step, usbOnline, via]);

  useEffect(() => {
    if (usbOnline || !usbSeen) {
      setUsbGone(false);
      return;
    }
    const t = setTimeout(() => setUsbGone(true), wireless.state === "running" ? 8000 : 2500);
    return () => clearTimeout(t);
  }, [usbOnline, usbSeen, wireless.state]);

  /* Once wireless mode is ready the cable is no longer needed, so unplugging is fine from then on. */
  const disconnected =
    usbGone && via === "usb" && step >= 2 && step <= WIRELESS_STEP && !(step === WIRELESS_STEP && (wireless.state === "ok" || ts.state === "ok"));

  /* An interrupted install or pairing becomes a clear failure with a restart button instead of hanging. */
  useEffect(() => {
    if (!disconnected) return;
    installRun.current++;
    wirelessRun.current++;
    setInstall((s) =>
      s.state === "running"
        ? { state: "failed", message: "Device disconnected during installation. Plug the cable back in and restart the installation." }
        : s,
    );
    setWireless((s) =>
      s.state === "running"
        ? { state: "failed", message: "Device disconnected during wireless setup. Plug the cable back in and restart the setup." }
        : s,
    );
  }, [disconnected]);

  /** Restarts the adb server and polls once more, for when a device is not shown. */
  async function retryConnection() {
    setRetryingConn(true);
    try {
      await invoke("restart_adb_server");
    } catch (e) {
      console.error("restart_adb_server failed:", e);
    }
    await poll();
    setRetryingConn(false);
  }

  /* Step 3 installs as soon as the device is authorized, then moves on. */
  useEffect(() => {
    if (step !== 3 || installStarted.current || usb?.state !== "device") return;
    installStarted.current = true;
    void runInstall(usb.serial);
  }, [step, usb, runInstall]);

  /* A QR session lives only while the guide is open and the manual dialog is not. */
  useEffect(() => {
    if (!open || step !== 0 || !wirelessOpen || manualOpen || mdnsBlocked || usbBootstrapped.current) return;
    let cancelled = false;
    invoke<QrPairingSession>("initiate_qr_pairing")
      .then((s) => !cancelled && setQr(s))
      .catch((e) => !cancelled && setPairError(`Could not start pairing: ${errText(e)}`));
    return () => {
      cancelled = true;
      setQr(null);
      setPairEvent(null);
      void invoke("cancel_qr_pairing");
    };
  }, [open, step, wirelessOpen, manualOpen, qrNonce, mdnsBlocked]);

  /* Wireless pairing succeeded: identify the device and go straight to Finish. */
  const finishWireless = useCallback(async (endpoint: string) => {
    setWifiEndpoint(endpoint);
    setTransport("auto");
    setVia("wireless");
    try {
      const info = await invoke<WirelessDeviceInfo>("wireless_device_info", { endpoint });
      setDevice({ serial: info.serial, model: info.model, state: "device", is_wifi: true });
    } catch (e) {
      setDevice({ serial: endpoint, model: endpoint, state: "device", is_wifi: true });
      console.error("wireless_device_info failed:", e);
    }
    setStep(FINISH_STEP);
  }, []);

  useEffect(() => {
    if (!open) return;
    const subs = [
      listen<ProgressEvent>("install_progress", ({ payload }) => setInstallLog((l) => [...l, payload.message])),
      listen<ProgressEvent>("pair_progress", ({ payload }) => setPairLog((l) => [...l, payload.message])),
      listen<PairingEvent>("pairing_event", ({ payload }) => {
        setPairEvent(payload);
        if (payload.kind === "success") {
          if (payload.connect_endpoint) void finishWireless(payload.connect_endpoint);
          else {
            setPairError("Paired, but the connect address could not be found. Use Manual setup to enter it.");
            setManualOpen(true);
          }
        }
      }),
      /* Backend saw no mDNS answer for 30 s: QR pairing cannot work, so offer the manual route instead. */
      listen<string>("mdns_timeout", () => {
        if (usbBootstrapped.current || stepRef.current !== 0) return;
        setMdnsBlocked(true);
      }),
    ];
    return () => {
      subs.forEach((p) => void p.then((fn) => fn()));
    };
  }, [open, finishWireless]);

  /** Asks the device to open its Wireless debugging settings screen and shows the reply. */
  async function openWirelessSettings() {
    if (!contextSerial) return;
    try {
      toast.info(await invoke<string>("open_wireless_debugging_settings", { serial: contextSerial }));
    } catch (e) {
      toast.error(errText(e));
    }
  }

  /* Ask exactly this device to announce itself (notification + buzz) so the user knows it is the right one. */
  async function identify(e: AdbEntry) {
    setIdentifying(e.serial);
    try {
      toast.success(`${e.model}: ${await invoke<string>("identify_device", { serial: e.serial, name: e.model })}`);
    } catch (err) {
      toast.error(`Could not identify ${e.model}: ${errText(err)}`);
    } finally {
      setIdentifying(null);
    }
  }

  /** Saves the new device's profile (label, nickname, transport, Wi-Fi address) and closes the wizard. */
  async function finish() {
    const serial = device?.serial;
    if (!serial) return;
    setSaving(true);
    try {
      await invoke("save_device_profile", {
        serial,
        profile: {
          label: device.model,
          nickname: nickname.trim() || undefined,
          preferred_transport: transport,
          wifi_address: wifiEndpoint ?? undefined,
          wireless_configured: wifiEndpoint ? true : false,
        },
      });
      toast.success(`${nickname.trim() || device.model} added`);
      await onDeviceAdded(serial);
      onOpenChange(false);
    } catch (e) {
      toast.error(`Could not save device: ${errText(e)}`);
    } finally {
      setSaving(false);
    }
  }

  /** Updates whether more content is hidden below the visible part of the scroller (more than 4 px). */
  const measureScroll = useCallback(() => {
    const el = scrollerRef.current;
    if (el) setMoreBelow(el.scrollHeight - el.scrollTop - el.clientHeight > 4);
  }, []);

  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    measureScroll();
    const ro = new ResizeObserver(measureScroll);
    ro.observe(el);
    if (el.firstElementChild) ro.observe(el.firstElementChild);
    return () => ro.disconnect();
  }, [step, wirelessOpen, mdnsBlocked, manualOpen, measureScroll]);

  /* The device IP the router refused to route to, when the last Automatic setup failed that way. */
  const routerBlock =
    wireless.state === "failed" && wireless.message.startsWith(ROUTER_BLOCKED_PREFIX)
      ? wireless.message.slice(ROUTER_BLOCKED_PREFIX.length).split("|")[0]
      : null;
  const pairingActive = pairEvent?.kind === "pairing";
  const alreadyPaired = contextSerial !== null && wirelessSerials.includes(contextSerial);
  const usbPresent = usbEntries.length > 0;

  /* Next is always shown on USB steps; each step enables it once its own condition holds. */
  const nextEnabled =
    disconnected ? false : step === 1 ? true : step === 2 ? usb?.state === "device" : step === 3 ? install.state === "ok" : step === 4 ? wireless.state === "ok" || ts.state === "ok" : false;
  /** Next step; on Connect USB it also pins the chosen device. */
  const goNext = () => {
    if (step === 2 && usb) setChosenSerial(usb.serial);
    if (step >= 1 && step <= 4) setStep((step + 1) as Step);
  };
  /** Skips the wireless step: the device is saved with USB only. */
  const skipWireless = () => {
    setWirelessSkipped(true);
    setWifiEndpoint(null);
    setTransport("auto");
    setStep(FINISH_STEP);
  };
  /** Starts the USB flow with no device chosen yet. */
  const startUsb = () => {
    /* A new run starts with no device chosen; the picker (or the pinned serial) decides. */
    setPickedSerial(null);
    setChosenSerial(null);
    setVia("usb");
    setWirelessOpen(false);
    setStep(1);
  };
  /** Previous step; from Finish on the wireless path it returns to the landing screen. */
  const goBack = () => {
    if (step === FINISH_STEP && via === "wireless") {
      setWifiEndpoint(null);
      setDevice(null);
      setStep(0);
    } else setStep((s) => (s - 1) as Step);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        onOpenAutoFocus={(e) => e.preventDefault()}
        className="flex max-h-[92vh] w-[480px] max-w-[96vw] flex-col gap-0 overflow-hidden p-0 text-sm"
      >
          <header className="px-6 pb-2 pt-4">
            <DialogTitle className="text-xl font-medium leading-8 text-foreground">
              {reconfigure ? "Configure wireless mode" : "Add device"}
            </DialogTitle>
          </header>

          {step > 0 && via === "usb" && !reconfigure && (
          <ol className="flex gap-2 px-6 pb-4 pt-2">
            {STEPS.map((label, i) => {
              const n = i + 1;
              return (
                <li key={label} className="flex-1">
                  <div
                    className={cn(
                      "h-1 rounded-full transition-colors",
                      n <= step ? "bg-primary" : "bg-muted",
                    )}
                  />
                  <div className={cn("mt-2 truncate text-xs", n === step ? "font-medium text-foreground" : "text-muted-foreground")}>
                    {n}. {label}
                  </div>
                </li>
              );
            })}
          </ol>
          )}

          <div className="relative flex min-h-[260px] flex-1 flex-col overflow-hidden">
          <div ref={scrollerRef} onScroll={measureScroll} className="flex flex-1 flex-col overflow-y-auto px-6 py-4"><div className="my-auto space-y-4">
            {step === 0 && (
              <>
                <button
                  type="button"
                  onClick={startUsb}
                  className="flex w-full items-start gap-4 rounded-xl bg-primary px-5 py-5 text-left text-primary-foreground transition-opacity hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
                >
                  <Usb className="mt-0.5 size-6 shrink-0" />
                  <span className="min-w-0">
                    <span className="block text-base font-medium">Setup via USB (Recommended)</span>
                    <span className="mt-1 block text-xs leading-relaxed opacity-90">
                      Plug in your device. We will automatically install the app and configure wireless mode.
                    </span>
                  </span>
                </button>

                <div>
                  <button
                    type="button"
                    aria-expanded={wirelessOpen}
                    aria-controls="wizard-wireless"
                    onClick={() => setWirelessOpen((v) => !v)}
                    className="flex items-center gap-1.5 rounded-sm text-xs text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <ChevronDown className={cn("size-3.5 transition-transform", wirelessOpen && "rotate-180")} />
                    Don&rsquo;t have a USB cable? Set up wirelessly.
                  </button>

                  {wirelessOpen && (
                    <div id="wizard-wireless" className="mt-3 space-y-3 text-foreground">
                      <Note>
                        You must download and install the UScreen APK manually on your device first. Once installed,
                        pair it below.
                      </Note>

                      {mdnsBlocked ? (
                        <div
                          role="alert"
                          className="rounded-lg border border-warning bg-warning/20 px-3 py-2.5 text-sm font-medium leading-snug text-foreground"
                        >
                          ⚠️ Your router is blocking mDNS discovery, so QR pairing is unavailable. Use the pairing code
                          instead.
                        </div>
                      ) : (
                        <div className="space-y-3">
                          <Instructions
                            items={[
                              <>Connect the device to the <b className="text-foreground">same Wi-Fi network</b> as this computer.</>,
                              <>Open <b className="text-foreground">Developer options, Wireless debugging</b> and switch it on.</>,
                              <>Tap <b className="text-foreground">Pair device with QR code</b> and scan the code below.</>,
                            ]}
                          />
                          <div className="flex gap-4 rounded-lg border border-border bg-background p-3">
                            <div className="flex size-[164px] shrink-0 items-center justify-center rounded-lg bg-white p-2">
                              {qr ? (
                                <QRCodeSVG value={qr.qr_payload} size={148} level="M" fgColor="#111318" bgColor="#ffffff" />
                              ) : (
                                <LoaderCircle className="size-5 animate-spin text-muted-foreground" />
                              )}
                            </div>
                            <div className="flex min-w-0 flex-1 flex-col justify-between">
                              <div className="space-y-2">
                                <div>
                                  <div className="text-[11px] text-muted-foreground">Pairing code</div>
                                  <div className="font-mono text-lg tracking-widest text-foreground">{qr?.pairing_code ?? "------"}</div>
                                </div>
                                <div>
                                  <div className="text-[11px] text-muted-foreground">Service</div>
                                  <div className="truncate font-mono text-[11px] text-muted-foreground">{qr?.service_name ?? "-"}</div>
                                </div>
                              </div>
                              <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
                                {pairingActive || pairEvent?.kind === "searching" ? (
                                  <LoaderCircle className="size-3 animate-spin text-primary" />
                                ) : null}
                                <span className={cn(pairEvent?.kind === "error" && "font-medium text-destructive")}>
                                  {pairEvent?.message ?? "Starting"}
                                </span>
                              </div>
                            </div>
                          </div>
                          <div className="flex items-center justify-between">
                            {contextSerial ? (
                              <Button size="sm" variant="ghost" onClick={openWirelessSettings}><ExternalLink />
                                Open Developer options on device
                              </Button>
                            ) : (
                              <span />
                            )}
                            <Button size="sm" variant="ghost" onClick={() => setQrNonce((n) => n + 1)}>
                              Retry QR
                            </Button>
                          </div>
                        </div>
                      )}
                      {pairError && <Note tone="error">{pairError}</Note>}

                      {/* One action that follows the device: already paired, plugged in, or not. */}
                      <div className="flex items-center justify-between gap-3 rounded-lg border border-border bg-background px-3 py-2.5">
                        <div className="min-w-0 text-xs text-muted-foreground">
                          {alreadyPaired
                            ? "Wireless is already set up for this device."
                            : usbPresent
                              ? "A USB device is plugged in; no code needed."
                              : "No USB device detected."}
                        </div>
                        {alreadyPaired ? (
                          <Badge variant="outline" className="shrink-0 gap-1 border-success bg-success/15 text-success">
                            <BadgeCheck className="size-3.5" />
                            Paired
                          </Badge>
                        ) : usbPresent ? (
                          <Button size="sm" className="shrink-0" onClick={startUsb}>
                            <Usb />
                            Pair Automatically (via USB)
                          </Button>
                        ) : (
                          <Button size="sm" className="shrink-0" onClick={() => setManualOpen(true)}>
                            <Wifi />
                            Pair Manually (via Code)
                          </Button>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              </>
            )}

            {step === 1 && <DebugTimeline />}

            {disconnected && (
              <div role="alert" className="space-y-3 rounded-lg border border-destructive bg-destructive/15 px-4 py-4 text-destructive" data-testid="disconnect-banner">
                <div className="text-sm font-semibold leading-snug">
                  ⚠️ Device disconnected. Please plug the USB cable back in to continue.
                </div>
                <Button size="sm" variant="secondary" onClick={retryConnection} disabled={retryingConn}>
                  {retryingConn ? <LoaderCircle className="animate-spin" /> : <Usb />}
                  Retry Connection
                </Button>
              </div>
            )}

            {step === 2 && !disconnected && (
              <>
                <p className="text-muted-foreground">Plug your device into this computer with a USB data cable.</p>

                {/* More than one device on the bus: the user picks, nothing is guessed. */}
                {needsPick && !pickedSerial && (
                  <div role="group" aria-label="Choose the device to set up" className="space-y-2" data-testid="device-picker">
                    <div className="text-foreground">
                      {usbEntries.length} devices are plugged in. Which one do you want to set up?
                    </div>
                    <ul className="space-y-2">
                      {usbEntries.map((e) => {
                        const added = pairedSerials.includes(e.serial);
                        return (
                          <li key={e.serial} className="flex items-center gap-2 rounded-lg border border-border bg-background px-3 py-2">
                            <Smartphone className="size-4 shrink-0 text-muted-foreground" />
                            <div className="min-w-0 flex-1">
                              <div className="truncate font-medium text-foreground">{e.model}</div>
                              <div className="truncate font-mono text-[11px] text-muted-foreground">
                                {e.serial}
                                {e.state !== "device" ? ` · ${e.state === "unauthorized" ? "waiting for authorization" : e.state}` : ""}
                              </div>
                            </div>
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => identify(e)}
                              disabled={e.state !== "device" || identifying === e.serial}
                              title="Make this device show a notification and vibrate"
                            >
                              {identifying === e.serial ? <LoaderCircle className="animate-spin" /> : <Siren />}
                              Identify
                            </Button>
                            {added ? (
                              <Badge variant="outline" className="shrink-0 border-success bg-success/15 text-success">
                                Already added
                              </Badge>
                            ) : (
                              <Button size="sm" onClick={() => setPickedSerial(e.serial)}>
                                Setup this device
                              </Button>
                            )}
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                )}

                {/* One box for the whole sequence: no device, device seen but unauthorized, authorized. */}
                {!(needsPick && !pickedSerial) && (
                  <div
                    role="status"
                    aria-live="polite"
                    className={cn(
                      "flex min-h-14 items-center justify-between gap-3 rounded-lg px-3 py-2.5",
                      usb?.state === "device" ? "bg-success/15 text-foreground" : "bg-muted text-foreground",
                    )}
                  >
                    <div className="min-w-0">
                      <div className="font-medium">
                        {!usb
                          ? "Waiting for USB device..."
                          : usb.state === "device"
                            ? "Device authorized."
                            : "Waiting for authorization on device..."}
                      </div>
                      {usb && (
                        <div className="truncate font-mono text-[11px] text-muted-foreground">
                          {usb.model} · {usb.serial}
                        </div>
                      )}
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                      {usb && (
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => identify(usb)}
                          disabled={usb.state !== "device" || identifying === usb.serial}
                          title="Make this device show a notification and vibrate"
                        >
                          {identifying === usb.serial ? <LoaderCircle className="animate-spin" /> : <Siren />}
                          Identify
                        </Button>
                      )}
                      {usb?.state === "device" ? (
                        <Check className="size-5 text-success" />
                      ) : (
                        <LoaderCircle className="size-5 animate-spin text-primary" />
                      )}
                    </div>
                  </div>
                )}
                {needsPick && pickedSerial && (
                  <button
                    type="button"
                    onClick={() => setPickedSerial(null)}
                    className="rounded-sm text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    Choose a different device
                  </button>
                )}
                <ul className="list-inside list-disc space-y-1 rounded-lg bg-muted/50 px-3 py-2 text-[11px] leading-relaxed text-muted-foreground">
                  <li>Use a USB cable that supports data transfer (not a charge-only cable).</li>
                  <li>Unlock your device screen.</li>
                  <li>Tap &lsquo;Allow&rsquo; on the &lsquo;Allow USB debugging?&rsquo; prompt on your device.</li>
                  <li>Not seeing the prompt? You may not have USB debugging enabled. Go back to Step 1 to check.</li>
                  {!pinned && pairedSerials.length > 0 && <li>Devices that are already paired cannot be set up again here.</li>}
                </ul>
              </>
            )}

            {step === 3 && !disconnected && (
              <>
                <p className="text-muted-foreground">
                  Installing the UScreen app over USB{hostVersion ? ` (build ${hostVersion})` : ""}
                  {usb ? ` on ${usb.model} (${usb.serial})` : ""}.
                </p>
                <div role="status" aria-live="polite" className="space-y-2 rounded-lg border border-border bg-background px-3 py-3">
                  {install.state === "idle" && (
                    <div className="flex items-center gap-2 text-muted-foreground">
                      <Smartphone className="size-4" />
                      Waiting for an authorized device
                    </div>
                  )}
                  <StepLog lines={installLog} running={install.state === "running"} />
                  {install.state === "running" && installLog.length === 0 && (
                    <div className="flex items-center gap-2 text-foreground">
                      <LoaderCircle className="size-3.5 animate-spin text-primary" />
                      Checking the device
                    </div>
                  )}
                  {install.state === "ok" && (
                    <div className="flex items-center gap-2 text-success">
                      <Check className="size-4" />
                      App installed
                    </div>
                  )}
                  {install.state === "failed" && (
                    <div className="flex items-start gap-2 text-destructive">
                      <X className="mt-0.5 size-4 shrink-0" />
                      <span className="min-w-0 break-words">{install.message}</span>
                    </div>
                  )}
                </div>
                {install.state === "failed" && (
                  <Button size="sm" onClick={() => usb && runInstall(usb.serial)}
                    disabled={!usb || usb.state !== "device"}>
                    Restart installation
                  </Button>
                )}
              </>
            )}

            {step === WIRELESS_STEP && !disconnected && (
              <>
                <p className="text-muted-foreground">
                  Wireless setup is optional. It lets {usb ? usb.model : "your device"} stream without the cable later.
                  Pick the route that suits your network; you can switch between them at any time.
                </p>

                <div role="tablist" aria-label="Wireless setup method" className="grid grid-cols-2 gap-1 rounded-xl bg-muted p-1">
                  {(
                    [
                      { id: "auto", title: "Automatic Setup", sub: "Recommended", icon: Usb, ok: wireless.state === "ok" },
                      { id: "tailscale", title: "Tailscale", sub: "Advanced", icon: Network, ok: ts.state === "ok" },
                    ] as const
                  ).map(({ id, title, sub, icon: Icon, ok }) => {
                    const on = wirelessTab === id;
                    return (
                      <button
                        key={id}
                        type="button"
                        role="tab"
                        id={`wireless-tab-${id}`}
                        aria-selected={on}
                        aria-controls={`wireless-panel-${id}`}
                        onClick={() => setWirelessTab(id)}
                        className={cn(
                          "flex flex-col items-start gap-0.5 rounded-lg px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                          on ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
                        )}
                      >
                        <span className="flex w-full items-center gap-2 text-sm font-medium">
                          <Icon className="size-4 shrink-0" />
                          <span className="min-w-0 flex-1 truncate">{title}</span>
                          {id === "tailscale" && (
                            <Badge
                              variant="outline"
                              data-testid="tailscale-experimental"
                              className="shrink-0 border-warning bg-warning/15 px-1.5 py-0 text-[10px] font-bold uppercase tracking-wide text-warning"
                            >
                              Experimental
                            </Badge>
                          )}
                          {ok && <Check className="size-4 shrink-0 text-success" />}
                        </span>
                        <span className="text-[11px] opacity-80">Option {id === "auto" ? "A" : "B"} &middot; {sub}</span>
                      </button>
                    );
                  })}
                </div>

                {wirelessTab === "auto" && (
                  <div role="tabpanel" id="wireless-panel-auto" aria-labelledby="wireless-tab-auto" className="space-y-3">
                    <p className="text-xs leading-relaxed text-muted-foreground">
                      Uses the USB cable to switch Wi-Fi debugging on for you. Both the device and this computer must be
                      on the same Wi-Fi network.
                    </p>
                    {reconfigure && !usb && (
                      <Note tone="warning">Plug the device in over USB to configure wireless mode.</Note>
                    )}

                    {wireless.state !== "idle" && !routerBlock && (
                      <div role="status" aria-live="polite" className="space-y-2 rounded-lg border border-border bg-background px-3 py-3">
                        <StepLog lines={pairLog} running={wireless.state === "running"} />
                        {wireless.state === "running" && pairLog.length === 0 && (
                          <div className="flex items-center gap-2 text-foreground">
                            <LoaderCircle className="size-3.5 animate-spin text-primary" />
                            Starting ADB server
                          </div>
                        )}
                        {wireless.state === "ok" && (
                          <div className="flex items-center gap-2 text-success" data-testid="wireless-ok">
                            <Check className="size-4 shrink-0" />
                            <span>
                              Wireless mode ready at <span className="font-mono">{wifiEndpoint ?? ""}</span>
                            </span>
                          </div>
                        )}
                        {wireless.state === "failed" && (
                          <div className="flex items-start gap-2 text-warning">
                            <AlertTriangle className="mt-0.5 size-4 shrink-0" />
                            <span className="min-w-0 break-words">{wireless.message}</span>
                          </div>
                        )}
                      </div>
                    )}

                    {/* The router is the problem, not the user: say so, name the usual causes, offer the fix. */}
                    {routerBlock && (
                      <div
                        role="alert"
                        data-testid="router-blocked"
                        className="space-y-3 rounded-lg border border-warning/60 bg-warning/15 px-4 py-3 text-foreground"
                      >
                        <div className="flex items-center gap-2 text-sm font-semibold">
                          <AlertTriangle className="size-4 shrink-0 text-warning" />
                          Connection Blocked by Router
                        </div>
                        <p className="text-xs leading-relaxed">
                          Your computer can see the device via USB, but the Wi-Fi router is preventing them from talking to
                          each other. This is not your fault&mdash;many modern routers block device-to-device traffic for
                          security. It is usually caused by:
                        </p>
                        <ul className="list-inside list-disc space-y-1 text-xs leading-relaxed">
                          <li>
                            <b>MAC Randomization:</b> Your device is using a fake MAC address.
                          </li>
                          <li>
                            <b>AP Isolation:</b> Your router&rsquo;s guest network is blocking local traffic.
                          </li>
                        </ul>
                        <p className="text-[11px] text-muted-foreground">
                          Device address tried: <span className="font-mono">{routerBlock}</span>. Still stuck? The Tailscale tab
                          bypasses the router entirely.
                        </p>
                        <div className="flex flex-wrap gap-2">
                          <Button size="sm" variant="secondary" onClick={() => setMacOpen(true)}>
                            <ScanSearch />
                            How to fix MAC Randomization
                          </Button>
                          <Button
                            size="sm"
                            onClick={() => usb && runBootstrap(usb.serial)}
                            disabled={!usb || usb.state !== "device" || saving}
                          >
                            <RotateCcw />
                            Retry Setup
                          </Button>
                        </div>
                      </div>
                    )}

                    {/* Collapsed by default; the routerBlock alert above is the same advice, shown only after a failure. */}
                    <div className="rounded-lg border border-warning/50 bg-warning/10" data-testid="connection-failed-box">
                      <button
                        type="button"
                        aria-expanded={failedHelpOpen}
                        aria-controls="connection-failed-body"
                        onClick={() => setFailedHelpOpen((o) => !o)}
                        className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm font-medium text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      >
                        <AlertTriangle className="size-4 shrink-0 text-warning" />
                        <span className="flex-1">Connection Failed?</span>
                        <ChevronDown className={cn("size-4 shrink-0 transition-transform", failedHelpOpen && "rotate-180")} />
                      </button>
                      {failedHelpOpen && (
                        <div id="connection-failed-body" className="space-y-2 px-3 pb-3">
                          <p className="text-xs leading-relaxed text-muted-foreground">
                            If the PC cannot find your device, your tablet might be blocking it via MAC Randomization.
                          </p>
                          <Instructions
                            items={[
                              <>Open <b className="text-foreground">Settings</b> &rarr; <b className="text-foreground">Wi-Fi</b> and tap your network (the gear icon).</>,
                              <>Change <b className="text-foreground">Privacy</b> (or MAC address type) from &ldquo;Randomized MAC&rdquo; to &ldquo;Device MAC&rdquo;.</>,
                              <>Reconnect to the network, then click <b className="text-foreground">Retry Setup</b>.</>,
                            ]}
                          />
                        </div>
                      )}
                    </div>

                    {wireless.state !== "ok" && !routerBlock && (
                      <Button
                        size="sm"
                        onClick={() => usb && runBootstrap(usb.serial)}
                        disabled={!usb || usb.state !== "device" || wireless.state === "running" || saving}
                      >
                        {wireless.state === "failed" ? <RotateCcw /> : <Wifi />}
                        {wireless.state === "failed" ? "Retry Setup" : "Configure Wireless over USB"}
                      </Button>
                    )}
                  </div>
                )}

                {wirelessTab === "tailscale" && (
                  <div role="tabpanel" id="wireless-panel-tailscale" aria-labelledby="wireless-tab-tailscale" className="space-y-3">
                    <div className="flex items-center gap-2 rounded-lg border border-warning/60 bg-warning/15 px-3 py-2 text-xs text-warning">
                      <Badge variant="outline" className="shrink-0 border-warning px-1.5 py-0 text-[10px] font-bold uppercase tracking-wide text-warning">
                        Experimental
                      </Badge>
                      <span>Tailscale integration is in beta.</span>
                    </div>
                    <p className="text-xs leading-relaxed text-muted-foreground">
                      If your router blocks local traffic, use Tailscale to create a secure, direct tunnel. Requires a free
                      Tailscale account.
                    </p>
                    <Button size="sm" variant="secondary" onClick={() => void openUrl(TAILSCALE_DOWNLOAD_URL)}>
                      <Download />
                      Download Tailscale
                    </Button>

                    <div className="space-y-1">
                      <label htmlFor="ts-host-ip" className="text-xs text-muted-foreground">
                        Tailscale IP of this PC
                      </label>
                      <div className="flex gap-2">
                        <TextInput
                          id="ts-host-ip"
                          className="w-full flex-1 font-mono"
                          placeholder="100.x.y.z"
                          value={tsHostIp}
                          onChange={(v) => setTsHostIp(v)}
                        />
                        <Button size="sm" variant="ghost" onClick={() => void detectTailscaleIp(false)} title="Run tailscale ip -4">
                          <ScanSearch />
                          Detect
                        </Button>
                      </div>
                    </div>
                    <div className="space-y-1">
                      <label htmlFor="ts-target-ip" className="text-xs text-muted-foreground">
                        Tailscale IP of the target device
                      </label>
                      <TextInput
                        id="ts-target-ip"
                        className="w-full font-mono"
                        placeholder="100.x.y.z"
                        value={tsTargetIp}
                        onChange={(v) => setTsTargetIp(v)}
                      />
                      <p className="text-[11px] leading-relaxed text-muted-foreground">
                        Look it up in your Tailscale admin console (login.tailscale.com/admin/machines) or in the Tailscale app on
                        the device.{usb?.state === "device" ? " The cable stays plugged in so the app can open adb port 5555 for you." : ""}
                      </p>
                    </div>

                    {ts.state === "ok" && (
                      <Note tone="success">
                        <span className="flex items-center gap-2" data-testid="tailscale-ok">
                          <Check className="size-4 shrink-0 text-success" />
                          <span>
                            Connected through Tailscale at <span className="font-mono">{ts.message}</span>
                          </span>
                        </span>
                      </Note>
                    )}
                    {ts.state === "failed" && (
                      <div role="alert" data-testid="tailscale-failed" className="space-y-2 rounded-lg border border-warning/60 bg-warning/15 px-4 py-3 text-xs leading-relaxed text-foreground">
                        {ts.message === "tailscale" ? (
                          <>
                            <div className="flex items-center gap-2 text-sm font-semibold">
                              <AlertTriangle className="size-4 shrink-0 text-warning" />
                              Could not connect via Tailscale
                            </div>
                            <p>Please ensure:</p>
                            <ol className="list-inside list-decimal space-y-1">
                              <li>Tailscale is running on both devices.</li>
                              <li>Both devices are logged into the same account (Tailnet).</li>
                              <li>The IP addresses entered are correct.</li>
                            </ol>
                            {ts.detail && <p className="break-words font-mono text-[11px] text-muted-foreground">{ts.detail}</p>}
                          </>
                        ) : (
                          <span className="break-words">{ts.message}</span>
                        )}
                      </div>
                    )}
                    <Button
                      size="sm"
                      onClick={() => void runTailscale()}
                      disabled={!tsHostIp.trim() || !tsTargetIp.trim() || ts.state === "running" || saving}
                    >
                      {ts.state === "running" ? <LoaderCircle className="animate-spin" /> : <Network />}
                      {ts.state === "failed" ? "Retry via Tailscale" : "Connect via Tailscale"}
                    </Button>
                  </div>
                )}

                {!reconfigure && (
                  <div className="border-t border-border pt-3">
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={skipWireless}
                      disabled={wireless.state === "running" || ts.state === "running"}
                    >
                      Skip for now (USB Only)
                    </Button>
                  </div>
                )}
              </>
            )}

            {step === FINISH_STEP && (
              <>
                <Note tone={wifiEndpoint ? "success" : "neutral"}>
                  {wifiEndpoint ? (
                    <>
                      Connected over Wi-Fi at <span className="font-mono">{wifiEndpoint}</span>.
                    </>
                  ) : (
                    <>
                      Set up over USB only. {wirelessSkipped ? "Wireless setup was skipped." : "Wireless mode was not configured."}{" "}
                      Use <b>Configure Wireless Mode</b> on the device page whenever you want it.
                    </>
                  )}
                </Note>

                <div className="space-y-2">
                  <div className="text-foreground">How do you usually connect this device?</div>
                  <div role="radiogroup" aria-label="Preferred connection" className="grid grid-cols-3 gap-2">
                    {(
                      [
                        { id: "usb", title: "USB cable", desc: "Lowest latency, charges the device", icon: Usb },
                        { id: "auto", title: "Auto", desc: "USB first, Wi-Fi fallback", icon: Zap },
                        { id: "wifi", title: "Wi-Fi", desc: "No cable needed", icon: Wifi },
                      ] as const
                    ).map(({ id, title, desc, icon: Icon }) => {
                      const disabled = id === "wifi" && !wifiEndpoint;
                      const on = transport === id;
                      return (
                        <button
                          key={id}
                          type="button"
                          role="radio"
                          aria-checked={on}
                          disabled={disabled}
                          onClick={() => setTransport(id)}
                          className={cn(
                            "rounded-lg border p-3 text-left transition-colors disabled:opacity-40",
                            on
                              ? "border-primary bg-secondary text-secondary-foreground"
                              : "border-border bg-background text-foreground hover:bg-muted",
                          )}
                        >
                          <div className="flex items-center gap-2 font-medium">
                            <Icon className="size-4" />
                            {title}
                          </div>
                          <div className="mt-1 text-[11px] opacity-80">
                            {disabled ? "Not set up yet" : desc}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="space-y-1">
                  <label htmlFor="wizard-nick" className="text-muted-foreground">
                    Nickname (optional)
                  </label>
                  <TextInput
                    id="wizard-nick"
                    className="w-full"
                    placeholder={device?.model ?? "Tablet"}
                    value={nickname}
                    onChange={(v) => setNickname(v)}
                  />
                </div>
              </>
            )}
          </div>

          </div>
          {/* Fade at the bottom edge while there is more to scroll to. */}
          <div
            aria-hidden
            className={cn(
              "pointer-events-none absolute inset-x-0 bottom-0 h-12 bg-gradient-to-t from-background to-transparent transition-opacity",
              moreBelow ? "opacity-100" : "opacity-0",
            )}
          />
          </div>

          {step > 0 && (
          <footer className="flex items-center justify-between px-6 pb-6 pt-2">
            {reconfigure ? (
              <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
            ) : (
              <Button size="sm" variant="ghost" onClick={goBack}><ChevronLeft />
                Back
              </Button>
            )}

            <div className="flex items-center gap-2">
              {reconfigure ? null : step < FINISH_STEP ? (
                /* Disabled until this step has what it needs. */
                <Button size="sm" onClick={goNext} disabled={!nextEnabled}>
                  Next
                </Button>
              ) : (
                <Button size="sm" onClick={finish} disabled={saving || !device}><Check />
                  Save device
                </Button>
              )}
            </div>
          </footer>
          )}
          {step === 0 && <div className="h-4" />}
      </DialogContent>
      {macOpen && (
        <Dialog open onOpenChange={setMacOpen}>
          <DialogContent className="w-[440px] max-w-[96vw] gap-4 text-sm" onOpenAutoFocus={(e) => e.preventDefault()}>
            <DialogTitle className="flex items-center gap-2 text-lg font-medium text-foreground">
              <Wifi className="size-5 text-primary" />
              How to fix MAC Randomization
            </DialogTitle>
            <p className="text-xs leading-relaxed text-muted-foreground">
              Android hides your real hardware address from each Wi-Fi network, and some routers then refuse to pass traffic to
              that &ldquo;fake&rdquo; device. Switching it off for this one network fixes that.
            </p>
            <Instructions
              items={[
                <>Open <b className="text-foreground">Settings</b> on your device.</>,
                <>Go to <b className="text-foreground">Wi-Fi</b> &rarr; tap your network.</>,
                <>Change <b className="text-foreground">Privacy</b> from &ldquo;Randomized MAC&rdquo; to &ldquo;Device MAC&rdquo;.</>,
                <>Reconnect and click <b className="text-foreground">Retry</b>.</>,
              ]}
            />
            <div className="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setMacOpen(false)}>
                Close
              </Button>
              <Button
                size="sm"
                onClick={() => {
                  setMacOpen(false);
                  if (usb) void runBootstrap(usb.serial);
                }}
                disabled={!usb || usb.state !== "device"}
              >
                <RotateCcw />
                Retry Setup
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      )}
      {manualOpen && (
        <ManualPairDialog
          onClose={() => setManualOpen(false)}
          onPaired={async (endpoint) => {
            setManualOpen(false);
            await finishWireless(endpoint);
          }}
        />
      )}
    </Dialog>
  );
}
