import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronLeft,
  ExternalLink,
  KeyRound,
  Info,
  LoaderCircle,
  MousePointerClick,
  QrCode,
  Settings,
  Smartphone,
  ToggleRight,
  Usb,
  Wifi,
  X,
  type LucideIcon,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { toast } from "../lib/toast";
import type { AdbEntry, ManualPairResult, PairingEvent, QrPairingSession, WirelessDeviceInfo } from "../lib/types";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { TextInput } from "./ui/controls";

/* Step 0 is the landing screen; steps 1-4 are the USB flow (the wireless path jumps to Finish). */
const STEPS = ["Prepare Device", "Connect USB", "Install", "Finish"] as const;
type Step = 0 | 1 | 2 | 3 | 4;
type WirelessTab = "qr" | "manual";
type Via = "usb" | "wireless";
type InstallState = "idle" | "running" | "ok" | "failed";
type Transport = "usb" | "wifi";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  hostVersion?: string;
  onDeviceAdded: (serial: string) => void | Promise<void>;
}

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

function Note({ tone = "neutral", children }: { tone?: "neutral" | "warning" | "error" | "success"; children: ReactNode }) {
  const tones = {
    neutral: "bg-muted text-foreground",
    warning: "bg-warning/15 text-warning",
    error: "bg-destructive/15 text-destructive",
    success: "bg-success/15 text-success",
  };
  return <div className={cn("rounded-lg px-3 py-2 text-xs leading-relaxed", tones[tone])}>{children}</div>;
}

function Instructions({ items }: { items: ReactNode[] }) {
  return (
    <ol className="space-y-2 text-muted-foreground">
      {items.map((item, i) => (
        <li key={i} className="flex gap-2">
          <span className="flex size-4 shrink-0 items-center justify-center rounded-[2px] bg-muted font-mono text-[10px] text-foreground">
            {i + 1}
          </span>
          <span>{item}</span>
        </li>
      ))}
    </ol>
  );
}

const DEBUG_STEPS: { icon: LucideIcon; text: ReactNode }[] = [
  { icon: Settings, text: <>Open <b>Settings</b> on your device.</> },
  { icon: Info, text: <>Go to <b>About Phone</b> (or About Tablet).</> },
  { icon: MousePointerClick, text: <>Tap <b>Build Number</b> 7 times until it says &lsquo;You are a developer&rsquo;.</> },
  { icon: ToggleRight, text: <>Go back and open <b>Developer Options</b>, then toggle <b>USB Debugging</b> ON.</> },
];

/** Vertical timeline: a large icon per step joined by a rail. */
function DebugTimeline() {
  return (
    <ol>
      {DEBUG_STEPS.map(({ icon: Icon, text }, i) => (
        <li key={i} className="flex gap-3">
          <div className="flex flex-col items-center">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-secondary text-secondary-foreground">
              <Icon className="size-5" />
            </span>
            {i < DEBUG_STEPS.length - 1 && <span className="my-1 w-px flex-1 bg-border" />}
          </div>
          <div className={cn("min-w-0 flex-1 pt-2.5 leading-snug text-foreground", i < DEBUG_STEPS.length - 1 && "pb-6")}>
            <span className="mr-1.5 font-mono text-[11px] text-muted-foreground">{i + 1}.</span>
            {text}
          </div>
        </li>
      ))}
    </ol>
  );
}

export function AddDeviceWizard({ open, onOpenChange, hostVersion, onDeviceAdded }: Props) {
  const [step, setStep] = useState<Step>(0);
  const [via, setVia] = useState<Via>("usb");
  const [entries, setEntries] = useState<AdbEntry[]>([]);
  const [device, setDevice] = useState<AdbEntry | null>(null);

  const [install, setInstall] = useState<{ state: InstallState; message: string }>({ state: "idle", message: "" });

  const [qr, setQr] = useState<QrPairingSession | null>(null);
  const [pairEvent, setPairEvent] = useState<PairingEvent | null>(null);
  const [wirelessOpen, setWirelessOpen] = useState(false);
  const [tab, setTab] = useState<WirelessTab>("qr");
  const [qrNonce, setQrNonce] = useState(0);
  const [mdnsWarning, setMdnsWarning] = useState<string | null>(null);
  const [pairAddr, setPairAddr] = useState("");
  const [pairCode, setPairCode] = useState("");
  const [connectAddr, setConnectAddr] = useState("");
  const [pairBusy, setPairBusy] = useState(false);
  const [pairError, setPairError] = useState<string | null>(null);

  const [wifiEndpoint, setWifiEndpoint] = useState<string | null>(null);
  const [transport, setTransport] = useState<Transport>("usb");
  const [nickname, setNickname] = useState("");
  const [saving, setSaving] = useState(false);

  const usb = entries.find((e) => !e.is_wifi) ?? null;
  const installStarted = useRef(false);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [moreBelow, setMoreBelow] = useState(false);

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

  const runInstall = useCallback(async (serial: string) => {
    setInstall({ state: "running", message: "Installing over USB" });
    try {
      await invoke("install_apk_on_device", { serial });
      setInstall({ state: "ok", message: "App installed" });
    } catch (e) {
      setInstall({ state: "failed", message: errText(e) });
    }
  }, []);

  /* Step 3 installs as soon as the device is authorized, then moves on. */
  useEffect(() => {
    if (step !== 3 || installStarted.current || usb?.state !== "device") return;
    installStarted.current = true;
    void runInstall(usb.serial);
  }, [step, usb, runInstall]);

  useEffect(() => {
    if (step !== 3 || install.state !== "ok") return;
    const t = setTimeout(() => setStep(4), 700);
    return () => clearTimeout(t);
  }, [step, install.state]);

  /* A QR session lives only while the QR tab is on screen. */
  useEffect(() => {
    if (!open || step !== 0 || !wirelessOpen || tab !== "qr") return;
    let cancelled = false;
    setMdnsWarning(null);
    invoke<QrPairingSession>("initiate_qr_pairing")
      .then((s) => !cancelled && setQr(s))
      .catch((e) => !cancelled && setPairError(`Could not start pairing: ${errText(e)}`));
    return () => {
      cancelled = true;
      setQr(null);
      setPairEvent(null);
      void invoke("cancel_qr_pairing");
    };
  }, [open, step, wirelessOpen, tab, qrNonce]);

  /* Wireless pairing succeeded: identify the device and go straight to Finish. */
  const finishWireless = useCallback(async (endpoint: string) => {
    setWifiEndpoint(endpoint);
    setTransport("wifi");
    setVia("wireless");
    try {
      const info = await invoke<WirelessDeviceInfo>("wireless_device_info", { endpoint });
      setDevice({ serial: info.serial, model: info.model, state: "device", is_wifi: true });
    } catch (e) {
      setDevice({ serial: endpoint, model: endpoint, state: "device", is_wifi: true });
      console.error("wireless_device_info failed:", e);
    }
    setStep(4);
  }, []);

  useEffect(() => {
    if (!open) return;
    const subs = [
      listen<PairingEvent>("pairing_event", ({ payload }) => {
        setPairEvent(payload);
        if (payload.kind === "success") {
          if (payload.connect_endpoint) void finishWireless(payload.connect_endpoint);
          else {
            setPairError("Paired, but the connect address could not be found. Enter it under Manual Pairing.");
            setTab("manual");
          }
        }
      }),
      /* Backend gave up on mDNS after 30 s: warn and move the user to Manual Pairing. */
      listen<string>("mdns_timeout", ({ payload }) => {
        setMdnsWarning(payload);
        setTab("manual");
      }),
    ];
    return () => {
      subs.forEach((p) => void p.then((fn) => fn()));
    };
  }, [open, finishWireless]);

  async function openWirelessSettings() {
    try {
      toast.info(await invoke<string>("open_wireless_debugging_settings", { serial: device?.serial ?? null }));
    } catch (e) {
      toast.error(errText(e));
    }
  }

  async function manualPair() {
    setPairError(null);
    setPairBusy(true);
    try {
      const res = await invoke<ManualPairResult>("manual_adb_pair", {
        pairEndpoint: pairAddr,
        code: pairCode,
        connectEndpoint: connectAddr.trim(),
      });
      if (res.connect_endpoint) await finishWireless(res.connect_endpoint);
      else setPairError("Paired, but adb could not connect to that Connect IP:Port.");
    } catch (e) {
      setPairError(errText(e));
    } finally {
      setPairBusy(false);
    }
  }

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
  }, [step, wirelessOpen, tab, measureScroll]);

  const pairingActive = pairEvent?.kind === "pairing";

  /* Next is always shown on USB steps; each step enables it once its own condition holds. */
  const nextEnabled = step === 1 ? true : step === 2 ? usb?.state === "device" : step === 3 ? install.state === "ok" : false;
  const goNext = () => {
    if (step === 1 || step === 2 || step === 3) setStep((step + 1) as Step);
  };
  const startUsb = () => {
    setVia("usb");
    setWirelessOpen(false);
    setStep(1);
  };
  const goBack = () => {
    if (step === 4 && via === "wireless") {
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
            <DialogTitle className="text-xl font-medium leading-8 text-foreground">Add device</DialogTitle>
          </header>

          {step > 0 && via === "usb" && (
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
                  <div className={cn("mt-2 text-xs", n === step ? "font-medium text-foreground" : "text-muted-foreground")}>
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
                    <div id="wizard-wireless" className="mt-3 space-y-3">
                      <Note>
                        You must download and install the UScreen APK manually on your device first. Once installed,
                        use one of the methods below.
                      </Note>

                      <div role="tablist" aria-label="Wireless pairing method" className="grid grid-cols-2 gap-1 rounded-lg bg-muted p-1">
                        {(
                          [
                            { id: "qr", title: "QR Code", icon: QrCode },
                            { id: "manual", title: "Manual Pairing", icon: KeyRound },
                          ] as const
                        ).map(({ id, title, icon: Icon }) => (
                          <button
                            key={id}
                            type="button"
                            role="tab"
                            aria-selected={tab === id}
                            onClick={() => setTab(id)}
                            className={cn(
                              "flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-colors",
                              tab === id ? "bg-background text-foreground shadow-sm" : "text-muted-foreground hover:text-foreground",
                            )}
                          >
                            <Icon className="size-3.5" />
                            {title}
                          </button>
                        ))}
                      </div>

                      {mdnsWarning && (
                        <Note tone="warning">
                          <span className="flex items-start gap-2">
                            <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
                            <span>{mdnsWarning}</span>
                          </span>
                        </Note>
                      )}

                      {tab === "qr" && (
                        <div className="space-y-3">
                          <Instructions
                            items={[
                              <>Connect the device to the <b>same Wi-Fi network</b> as this computer.</>,
                              <>Open <b>Developer options, Wireless debugging</b> and switch it on.</>,
                              <>Tap <b>Pair device with QR code</b> and scan the code below.</>,
                            ]}
                          />
                          <div className="flex gap-4 rounded-lg border border-border bg-background p-3">
                            <div className="flex size-[164px] shrink-0 items-center justify-center rounded-lg bg-white p-2">
                              {qr ? (
                                <QRCodeSVG value={qr.qr_payload} size={148} level="M" fgColor="#111318" bgColor="#ffffff" />
                              ) : (
                                <LoaderCircle className="size-5 animate-spin text-background" />
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
                                <span className={cn(pairEvent?.kind === "error" && "text-destructive")}>
                                  {pairEvent?.message ?? "Starting"}
                                </span>
                              </div>
                            </div>
                          </div>
                          <div className="flex items-center justify-between">
                            <Button size="sm" variant="ghost" onClick={openWirelessSettings}><ExternalLink />
                              Open Developer options on device
                            </Button>
                            <Button size="sm" variant="ghost" onClick={() => setQrNonce((n) => n + 1)}>
                              Retry QR
                            </Button>
                          </div>
                        </div>
                      )}

                      {tab === "manual" && (
                        <div className="space-y-2 rounded-lg border border-border bg-background p-3">
                          <p className="text-[11px] leading-relaxed text-muted-foreground">
                            On the device choose <b>Wireless debugging, Pair device with pairing code</b> and enter the
                            address and code it shows. The <b>Connect IP:Port</b> is different: find it on the main
                            Wireless Debugging screen of the tablet, under &lsquo;IP address &amp; Port&rsquo;.
                          </p>
                          <div className="grid grid-cols-[1fr_120px] gap-2">
                            <TextInput
                              aria-label="Pairing IP:Port"
                              className="w-full font-mono"
                              placeholder="Pairing IP:Port"
                              value={pairAddr}
                              onChange={(v) => setPairAddr(v)}
                            />
                            <TextInput
                              aria-label="6-digit pairing code"
                              className="w-full font-mono"
                              placeholder="6-digit code"
                              inputMode="numeric"
                              maxLength={6}
                              value={pairCode}
                              onChange={(v) => setPairCode(v.replace(/\D/g, ""))}
                            />
                          </div>
                          <TextInput
                            aria-label="Connect IP:Port (required)"
                            className="w-full font-mono"
                            placeholder="Connect IP:Port (required)"
                            value={connectAddr}
                            onChange={(v) => setConnectAddr(v)}
                          />
                          <Button
                            size="sm"
                            onClick={manualPair}
                            disabled={pairBusy || !pairAddr.trim() || pairCode.length !== 6 || !connectAddr.trim()}
                          >
                            {pairBusy ? <LoaderCircle className="animate-spin" /> : <Wifi />}
                            Pair and connect
                          </Button>
                        </div>
                      )}
                      {pairError && <Note tone="error">{pairError}</Note>}
                    </div>
                  )}
                </div>
              </>
            )}

            {step === 1 && <DebugTimeline />}

            {step === 2 && (
              <>
                <p className="text-muted-foreground">Plug your device into this computer with a USB data cable.</p>
                {/* One box for the whole sequence: no device, device seen but unauthorized, authorized. */}
                <div
                  role="status"
                  aria-live="polite"
                  className={cn(
                    "flex min-h-14 items-center justify-between gap-3 rounded-lg px-3 py-2.5",
                    usb?.state === "device" ? "bg-success/15 text-success" : "bg-muted text-foreground",
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
                  {usb?.state === "device" ? (
                    <Check className="size-5 shrink-0" />
                  ) : (
                    <LoaderCircle className="size-5 shrink-0 animate-spin text-primary" />
                  )}
                </div>
                <ul className="list-inside list-disc space-y-1 rounded-lg bg-muted/50 px-3 py-2 text-[11px] leading-relaxed text-muted-foreground">
                  <li>Use a USB cable that supports data transfer (not a charge-only cable).</li>
                  <li>Unlock your device screen.</li>
                  <li>Tap &lsquo;Allow&rsquo; on the &lsquo;Allow USB debugging?&rsquo; prompt on your device.</li>
                  <li>Not seeing the prompt? You may not have USB debugging enabled. Go back to Step 1 to check.</li>
                </ul>
              </>
            )}

            {step === 3 && (
              <>
                <p className="text-muted-foreground">
                  Installing the UScreen app over USB{hostVersion ? ` (build ${hostVersion})` : ""}.
                </p>
                <div className="flex items-center gap-3 rounded-lg border border-border bg-background px-3 py-3">
                  {install.state === "running" && <LoaderCircle className="size-4 animate-spin text-primary" />}
                  {install.state === "ok" && <Check className="size-4 text-success" />}
                  {install.state === "failed" && <X className="size-4 text-destructive" />}
                  {install.state === "idle" && <Smartphone className="size-4 text-muted-foreground" />}
                  <span className="text-foreground">
                    {install.state === "idle" ? "Waiting for an authorized device" : install.message}
                  </span>
                </div>
                {install.state === "failed" && (
                  <Button size="sm" onClick={() => usb && runInstall(usb.serial)}
                    disabled={!usb || usb.state !== "device"}>
                    Retry install
                  </Button>
                )}
              </>
            )}

            {step === 4 && (
              <>
                <Note tone="success">
                  {wifiEndpoint ? (
                    <>
                      Paired and connected over Wi-Fi at <span className="font-mono">{wifiEndpoint}</span>.
                    </>
                  ) : (
                    <>Set up over USB. The host enables wireless mode on its own while the cable is plugged in.</>
                  )}
                </Note>

                <div className="space-y-2">
                  <div className="text-foreground">How do you usually connect this device?</div>
                  <div role="radiogroup" aria-label="Preferred connection" className="grid grid-cols-2 gap-2">
                    {(
                      [
                        { id: "usb", title: "USB cable", desc: "Lowest latency, charges the device", icon: Usb },
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
            <Button size="sm" variant="ghost" onClick={goBack}><ChevronLeft />
              Back
            </Button>

            <div className="flex items-center gap-2">
              {step === 3 && install.state !== "ok" && (
                <Button size="sm" variant="ghost" onClick={() => setStep(4)}>
                  Skip install
                </Button>
              )}
              {step < 4 ? (
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
    </Dialog>
  );
}
