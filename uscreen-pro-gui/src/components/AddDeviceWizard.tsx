import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, ChevronLeft, ExternalLink, LoaderCircle, RefreshCw, Smartphone, Usb, Wifi, X } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { toast } from "../lib/toast";
import type { AdbEntry, ManualPairResult, PairingEvent, QrPairingSession } from "../lib/types";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { TextInput } from "./ui/controls";

const STEPS = ["Connect", "Debugging", "Install", "Wireless", "Finish"] as const;
type Step = 1 | 2 | 3 | 4 | 5;
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

export function AddDeviceWizard({ open, onOpenChange, hostVersion, onDeviceAdded }: Props) {
  const [step, setStep] = useState<Step>(1);
  const [entries, setEntries] = useState<AdbEntry[]>([]);
  const [device, setDevice] = useState<AdbEntry | null>(null);

  const [install, setInstall] = useState<{ state: InstallState; message: string }>({ state: "idle", message: "" });

  const [qr, setQr] = useState<QrPairingSession | null>(null);
  const [pairEvent, setPairEvent] = useState<PairingEvent | null>(null);
  const [manualOpen, setManualOpen] = useState(false);
  const [pairAddr, setPairAddr] = useState("");
  const [pairCode, setPairCode] = useState("");
  const [connectAddr, setConnectAddr] = useState("");
  const [needConnect, setNeedConnect] = useState(false);
  const [pairBusy, setPairBusy] = useState(false);
  const [pairError, setPairError] = useState<string | null>(null);

  const [wifiEndpoint, setWifiEndpoint] = useState<string | null>(null);
  const [transport, setTransport] = useState<Transport>("usb");
  const [nickname, setNickname] = useState("");
  const [saving, setSaving] = useState(false);

  const usb = entries.find((e) => !e.is_wifi) ?? null;
  const installStarted = useRef(false);

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
    if (usb) setDevice(usb);
  }, [usb]);

  /* Steps 1 and 2 advance on their own as the ADB state changes. */
  useEffect(() => {
    if (!open) return;
    if (step === 1 && usb) setStep(usb.state === "device" ? 3 : 2);
    else if (step === 2 && usb?.state === "device") setStep(3);
    else if (step === 2 && !usb) setStep(1);
  }, [open, step, usb]);

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

  /* Step 4: a QR session lives only while that step is on screen. */
  useEffect(() => {
    if (!open || step !== 4) return;
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
  }, [open, step]);

  useEffect(() => {
    if (!open) return;
    const unlisten = listen<PairingEvent>("pairing_event", ({ payload }) => {
      setPairEvent(payload);
      if (payload.kind === "stalled" || payload.kind === "timeout" || payload.kind === "error") setManualOpen(true);
      if (payload.kind === "success") {
        setWifiEndpoint(payload.connect_endpoint ?? null);
        setTransport(payload.connect_endpoint ? "wifi" : "usb");
        setStep(5);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [open]);

  async function openDevOptions() {
    try {
      toast.info(await invoke<string>("open_developer_options", { serial: device?.serial ?? null }));
    } catch (e) {
      toast.error(`The device refused the command: ${errText(e)}`);
    }
  }

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
      if (needConnect) {
        const endpoint = await invoke<string>("manual_adb_connect", { endpoint: connectAddr });
        setWifiEndpoint(endpoint);
        setTransport("wifi");
        setStep(5);
        return;
      }
      const res = await invoke<ManualPairResult>("manual_adb_pair", {
        pairEndpoint: pairAddr,
        code: pairCode,
        connectEndpoint: connectAddr || null,
      });
      if (res.connect_endpoint) {
        setWifiEndpoint(res.connect_endpoint);
        setTransport("wifi");
        setStep(5);
      } else {
        setNeedConnect(true);
        toast.success("Paired. Enter the connect address shown on the Wireless debugging screen.");
      }
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

  const pairingActive = pairEvent?.kind === "pairing";

  /* Next is always shown; each step enables it once its own condition holds. */
  const deviceConnected = usb !== null;
  const nextEnabled =
    step === 1 ? deviceConnected : step === 2 ? usb?.state === "device" : step === 3 ? install.state === "ok" : step === 4 ? wifiEndpoint !== null : false;
  const goNext = () => {
    if (step === 1 && usb) setStep(usb.state === "device" ? 3 : 2);
    else if (step === 2 || step === 3 || step === 4) setStep((step + 1) as Step);
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

          <div className="flex min-h-[260px] flex-1 flex-col overflow-y-auto px-6 py-4"><div className="my-auto space-y-4">
            {step === 1 && (
              <>
                <p className="text-muted-foreground">Plug your Android device into this computer with a USB data cable.</p>
                <div className="flex items-center justify-between rounded-lg border border-border bg-background px-3 py-2">
                  <div className="flex items-center gap-2">
                    <Usb className="size-4 text-muted-foreground" />
                    <div>
                      <div className="text-foreground">{usb ? usb.model : "Waiting for a USB device"}</div>
                      <div className="font-mono text-[11px] text-muted-foreground">{usb ? usb.serial : "Polling ADB"}</div>
                    </div>
                  </div>
                  {!usb && <LoaderCircle className="size-4 animate-spin text-primary" />}
                </div>
                {/* surface-container-high on the card, on-surface text: >= 4.5:1 (measured in the notes). */}
                <div className="rounded-lg bg-muted px-3 py-2 text-xs leading-relaxed text-foreground">
                  <div className="mb-1 font-medium">Nothing showing up?</div>
                  <ul className="list-inside list-disc space-y-1">
                    <li>Use a USB cable that carries data, not a charge-only one.</li>
                    <li>Unlock the device screen and accept any USB prompt.</li>
                    <li>If USB debugging was never enabled: Settings, About phone, tap Build number seven times.</li>
                    <li>Then turn on USB debugging under Developer options.</li>
                  </ul>
                </div>
              </>
            )}

            {step === 2 && (
              <>
                <p className="text-muted-foreground">
                  The device is connected but has not authorized this computer
                  {usb ? <> (state: <span className="font-mono text-warning">{usb.state}</span>)</> : null}.
                </p>
                <Instructions
                  items={[
                    <>Unlock the device and look for the <b>Allow USB debugging?</b> prompt.</>,
                    <>Tick <b>Always allow from this computer</b>, then tap <b>Allow</b>.</>,
                    <>No prompt? Open Developer options and toggle <b>USB debugging</b> off and on.</>,
                  ]}
                />
                <div className="flex items-center gap-2">
                  <Button size="sm" variant="secondary" onClick={openDevOptions}><ExternalLink />
                    Open Developer options on device
                  </Button>
                  <Button size="sm" variant="ghost" onClick={poll}><RefreshCw />
                    Check again
                  </Button>
                </div>
                <Note tone="warning">
                  Android only accepts ADB commands from authorized computers, so the button above can fail while the
                  state is unauthorized. If it does, follow the steps by hand. This screen continues automatically.
                </Note>
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
                      <span className={cn(pairEvent?.kind === "error" && "text-destructive", pairEvent?.kind === "timeout" && "text-warning")}>
                        {pairEvent?.message ?? "Starting"}
                      </span>
                    </div>
                  </div>
                </div>

                <div className="flex items-center justify-between">
                  <Button size="sm" variant="ghost" onClick={openWirelessSettings}><ExternalLink />
                    Open Developer options on device
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setManualOpen((v) => !v)}>
                    {manualOpen ? "Hide manual pairing" : "Pair with a code instead"}
                  </Button>
                </div>

                {manualOpen && (
                  <div className="space-y-2 rounded-lg border border-border bg-background p-3">
                    <p className="text-[11px] text-muted-foreground">
                      On the device choose <b>Pair device with pairing code</b>, then enter the address and code it
                      shows. The connect address is the IP and port on the main Wireless debugging screen; leave it
                      empty to look it up automatically.
                    </p>
                    <div className="grid grid-cols-[1fr_120px] gap-2">
                      <TextInput
                        aria-label="Pairing address"
                        className="w-full font-mono"
                        placeholder="Pairing IP:port"
                        value={pairAddr}
                        disabled={needConnect}
                        onChange={(v) => setPairAddr(v)}
                      />
                      <TextInput
                        aria-label="Pairing code"
                        className="w-full font-mono"
                        placeholder="6-digit code"
                        maxLength={6}
                        value={pairCode}
                        disabled={needConnect}
                        onChange={(v) => setPairCode(v.replace(/\D/g, ""))}
                      />
                    </div>
                    <TextInput
                      aria-label="Connect address"
                      className={cn("w-full font-mono", needConnect && "border-primary")}
                      placeholder={needConnect ? "Connect IP:port (required)" : "Connect IP:port (optional)"}
                      value={connectAddr}
                      onChange={(v) => setConnectAddr(v)}
                    />
                    <div className="flex items-center gap-3">
                      <Button size="sm"
                        onClick={manualPair}
                        disabled={
                          pairBusy ||
                          (needConnect ? !connectAddr.trim() : !pairAddr.trim() || pairCode.length !== 6)
                        }><Wifi />
                        {needConnect ? "Connect" : "Pair"}
                      </Button>
                    </div>
                  </div>
                )}
                {pairError && <Note tone="error">{pairError}</Note>}
              </>
            )}

            {step === 5 && (
              <>
                <Note tone="success">
                  {wifiEndpoint ? (
                    <>
                      Paired and connected over Wi-Fi at <span className="font-mono">{wifiEndpoint}</span>.
                    </>
                  ) : (
                    <>Set up over USB. Wireless pairing was skipped.</>
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
                            {disabled ? "Pair over Wi-Fi first" : desc}
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

          <footer className="flex items-center justify-between px-6 pb-6 pt-2">
            <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>

            <div className="flex items-center gap-2">
              {step > 1 && step < 5 && (
                <Button size="sm" variant="ghost" onClick={() => setStep((s) => (s - 1) as Step)}><ChevronLeft />
                  Back
                </Button>
              )}
              {step === 3 && install.state !== "ok" && (
                <Button size="sm" variant="ghost" onClick={() => setStep(4)}>
                  Skip install
                </Button>
              )}
              {step === 4 && (
                <Button size="sm" variant="ghost" onClick={() => setStep(5)}>
                  Skip, use USB only
                </Button>
              )}
              {step < 5 ? (
                /* Visible from the first screen, disabled until this step has what it needs. */
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
      </DialogContent>
    </Dialog>
  );
}
