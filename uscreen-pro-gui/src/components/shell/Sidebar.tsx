/**
 * Sidebar.tsx: left navigation. Lists the views, the connected devices with their state, and the
 * start/stop button of the host daemon. Pure presentation: all state and callbacks arrive as props from App.tsx.
 */
import { Activity, FlaskConical, LoaderCircle, Play, Plus, Sliders, Square, Tablet, Usb, WifiOff } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import type { AppConfig, ConnectedDevice, View } from "../../lib/types";

/**
 * Props of the sidebar: current view, connected devices, config, daemon state and navigation callbacks.
 */
interface Props {
  view: View;
  devices: ConnectedDevice[];
  config: AppConfig | null;
  /** Serials whose pipeline is being respawned; "*" stands for every device. */
  restarting: ReadonlySet<string>;
  daemonRunning: boolean;
  daemonPid?: number;
  daemonBusy: "starting" | "stopping" | null;
  onToggleDaemon: () => void;
  onSelect: (v: View) => void;
  /** Open the wizard; `serial` pins it to the clicked new device. */
  onAddDevice: (serial?: string) => void;
}

/** One row of the sidebar with an icon, label, optional badge and status. */
function NavItem({
  selected,
  icon,
  label,
  badge,
  status,
  onClick,
}: {
  selected: boolean;
  icon: ReactNode;
  label: string;
  badge?: string;
  /** Second line under the label: what the device is doing right now. */
  status?: { text: string; busy: boolean };
  onClick: () => void;
}) {
  return (
    <Button
      variant="ghost"
      size="sm"
      aria-current={selected ? "page" : undefined}
      onClick={onClick}
      className={cn(
        "w-full justify-start gap-3 px-3 font-normal",
        status && "h-auto min-h-8 py-1.5",
        selected && "bg-secondary font-medium text-secondary-foreground hover:bg-secondary",
      )}
    >
      {icon}
      <span className="min-w-0 flex-1 text-left">
        <span className="block truncate">{label}</span>
        {status && (
          <span className="flex items-center gap-1 text-[11px] font-normal text-warning" data-testid="device-status">
            {status.busy ? <LoaderCircle className="size-3 animate-spin" /> : <span className="size-1.5 rounded-full bg-warning" />}
            <span className="truncate" title={status.busy ? "Restarting pipeline, updating APK or waiting for the stream" : undefined}>{status.text}</span>
          </span>
        )}
      </span>
      {badge && <span className="text-[11px] text-muted-foreground">{badge}</span>}
    </Button>
  );
}

/** Small caption above a group of sidebar rows. */
function SectionLabel({ children }: { children: ReactNode }) {
  return <div className="px-3 pb-1 pt-2 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">{children}</div>;
}

/** What a paired device is busy with, in priority order: APK install, pipeline restart, pause. */
function deviceStatus(d: ConnectedDevice, restarting: ReadonlySet<string>): { text: string; busy: boolean } | undefined {
  if (d.install?.state === "installing") return { text: "Updating APK...", busy: true };
  if (d.install?.state === "queued") return { text: "Update queued...", busy: false };
  if (restarting.has(d.serial) || restarting.has("*")) return { text: "Restarting...", busy: true };
  if (d.paused) return { text: "Paused", busy: false };
  return undefined;
}

/**
 * Left navigation: global pages, one entry per paired device (with live status text) and the
 * daemon state at the bottom.
 */
export function Sidebar({
  view,
  devices,
  config,
  restarting,
  daemonRunning,
  daemonPid,
  daemonBusy,
  onToggleDaemon,
  onSelect,
  onAddDevice,
}: Props) {
  const selectedSerial = typeof view === "object" ? view.device : null;
  const busyLabel = daemonBusy === "starting" ? "Starting" : "Stopping";
  // Paired = has a [tablets.<serial>] entry, whether it is plugged in, on Wi-Fi or offline.
  // New = connected but not paired yet.
  const pairedDevices = devices.filter((d) => d.paired);
  const newDevices = devices.filter((d) => !d.paired && d.transport !== "Disconnected");

  return (
    <nav aria-label="UScreen navigation" className="flex w-60 shrink-0 flex-col gap-1 border-r border-border p-3">
      {/* One call-to-action is the daemon status: green Start when stopped, red Stop when running. */}
      <Button
        variant={daemonRunning ? "destructive" : "default"}
        size="sm"
        className="mb-4 w-full"
        disabled={daemonBusy !== null}
        title={daemonBusy ? busyLabel : daemonRunning ? `Running, PID ${daemonPid ?? "?"}` : "Stopped"}
        onClick={onToggleDaemon}
      >
        {daemonBusy ? <LoaderCircle className="animate-spin" /> : daemonRunning ? <Square /> : <Play />}
        {daemonRunning ? "Stop Daemon" : "Start Daemon"}
      </Button>

      <NavItem selected={view === "settings"} icon={<Sliders />} label="Settings" onClick={() => onSelect("settings")} />
      <NavItem selected={view === "advanced"} icon={<FlaskConical />} label="Advanced" onClick={() => onSelect("advanced")} />
      <NavItem selected={view === "health"} icon={<Activity />} label="System health" onClick={() => onSelect("health")} />

      <Separator className="my-2" />
      <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
        {/* Paired devices always come first, and the section is kept (with "None") when empty. */}
        <SectionLabel>Paired Devices ({pairedDevices.length})</SectionLabel>
        {pairedDevices.length === 0 && (
          <div data-testid="paired-none" className="px-3 pb-2 text-xs text-muted-foreground/70">
            None
          </div>
        )}
        {pairedDevices.map((d) => {
          const name = config?.tablets[d.serial]?.nickname?.trim() || d.model || d.serial;
          const online = d.transport !== "Disconnected" && d.state === "device";
          return (
            <NavItem
              key={d.serial}
              selected={selectedSerial === d.serial}
              icon={online ? <Tablet /> : <WifiOff />}
              label={name}
              badge={d.transport === "Disconnected" ? "off" : d.transport}
              status={deviceStatus(d, restarting)}
              onClick={() => onSelect({ device: d.serial })}
            />
          );
        })}
        {newDevices.length > 0 && (
          <>
            <SectionLabel>New Devices ({newDevices.length})</SectionLabel>
            {newDevices.map((d) => (
              <NavItem
                key={d.serial}
                selected={false}
                icon={<Usb />}
                label={d.model || d.serial}
                badge={d.transport}
                status={{ text: "Not set up. Click to add", busy: false }}
                onClick={() => onAddDevice(d.serial)}
              />
            ))}
          </>
        )}
      </div>

      <Button variant="secondary" size="sm" className="mt-2 w-full" onClick={() => onAddDevice()}>
        <Plus />
        Add device
      </Button>
    </nav>
  );
}
