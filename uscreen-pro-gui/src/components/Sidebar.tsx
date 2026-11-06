import { Activity, FlaskConical, LoaderCircle, Play, Plus, Sliders, Square, Tablet, WifiOff } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import type { AppConfig, ConnectedDevice, View } from "../lib/types";

interface Props {
  view: View;
  devices: ConnectedDevice[];
  config: AppConfig | null;
  daemonRunning: boolean;
  daemonPid?: number;
  daemonBusy: "starting" | "stopping" | null;
  onToggleDaemon: () => void;
  onSelect: (v: View) => void;
  onAddDevice: () => void;
}

function NavItem({
  selected,
  icon,
  label,
  badge,
  onClick,
}: {
  selected: boolean;
  icon: ReactNode;
  label: string;
  badge?: string;
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
        selected && "bg-secondary font-medium text-secondary-foreground hover:bg-secondary",
      )}
    >
      {icon}
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      {badge && <span className="text-[11px] text-muted-foreground">{badge}</span>}
    </Button>
  );
}

export function Sidebar({
  view,
  devices,
  config,
  daemonRunning,
  daemonPid,
  daemonBusy,
  onToggleDaemon,
  onSelect,
  onAddDevice,
}: Props) {
  const selectedSerial = typeof view === "object" ? view.device : null;
  const busyLabel = daemonBusy === "starting" ? "Starting" : "Stopping";

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
      <div className="px-3 pb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        Devices ({devices.length})
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
        {devices.length === 0 && <div className="px-3 py-2 text-muted-foreground">No devices yet</div>}
        {devices.map((d) => {
          const name = config?.tablets[d.serial]?.nickname?.trim() || d.model || d.serial;
          const online = d.transport !== "Disconnected" && d.state === "device";
          return (
            <NavItem
              key={d.serial}
              selected={selectedSerial === d.serial}
              icon={online ? <Tablet /> : <WifiOff />}
              label={name}
              badge={d.transport === "Disconnected" ? "off" : d.transport}
              onClick={() => onSelect({ device: d.serial })}
            />
          );
        })}
      </div>

      <Button variant="secondary" size="sm" className="mt-2 w-full" onClick={onAddDevice}>
        <Plus />
        Add device
      </Button>
    </nav>
  );
}
