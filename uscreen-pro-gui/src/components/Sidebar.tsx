import { Activity, LoaderCircle, Play, Plus, Sliders, Square } from "lucide-react";
import type { ReactNode } from "react";
import type { AppConfig, ConnectedDevice, View } from "../lib/types";
import { Button, cx } from "./ui/controls";

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

function NavItem({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cx(
        "flex h-8 w-full items-center gap-2 rounded px-2 text-left text-xs transition-[border-radius,background-color] duration-150 hover:rounded-lg",
        active
          ? "bg-primary-container font-medium text-on-primary-container"
          : "text-on-surface-variant hover:bg-surface-high",
      )}
    >
      {children}
    </button>
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
    <aside className="flex w-60 shrink-0 flex-col border-r border-outline-variant bg-surface-low">
      <div className="m-3 space-y-2.5 rounded-lg border border-outline-variant bg-surface-container p-3">
        <div className="flex items-center gap-2.5">
          <span
            className={cx("size-2.5 shrink-0 rounded-full", daemonRunning ? "bg-success" : "bg-error")}
            aria-hidden
          />
          <div className="min-w-0">
            <div className="text-xs font-semibold text-on-surface">Host daemon</div>
            <div className="truncate text-[11px] text-on-surface-variant">
              {daemonBusy ? busyLabel : daemonRunning ? `Running, PID ${daemonPid ?? "?"}` : "Stopped"}
            </div>
          </div>
        </div>
        <Button
          variant={daemonRunning ? "tonal" : "filled"}
          className="w-full"
          disabled={daemonBusy !== null}
          onClick={onToggleDaemon}
        >
          {daemonBusy ? (
            <LoaderCircle className="size-3.5 animate-spin" />
          ) : daemonRunning ? (
            <Square className="size-3.5" />
          ) : (
            <Play className="size-3.5" />
          )}
          {daemonRunning ? "Stop daemon" : "Start daemon"}
        </Button>
      </div>

      <nav className="space-y-0.5 px-3 pb-2">
        <NavItem active={view === "settings"} onClick={() => onSelect("settings")}>
          <Sliders className="size-4" />
          Settings
        </NavItem>
        <NavItem active={view === "health"} onClick={() => onSelect("health")}>
          <Activity className="size-4" />
          System health
        </NavItem>
      </nav>

      <div className="flex items-center justify-between px-4 pb-1 pt-3">
        <span className="text-xs font-semibold text-primary">Devices</span>
        <span className="font-mono text-[11px] text-outline">{devices.length}</span>
      </div>

      <div className="min-h-0 flex-1 space-y-0.5 overflow-y-auto px-3">
        {devices.length === 0 && <div className="px-2 py-2 text-outline">No devices yet</div>}
        {devices.map((d) => {
          const name = config?.tablets[d.serial]?.nickname?.trim() || d.model || d.serial;
          const online = d.transport !== "Disconnected" && d.state === "device";
          return (
            <NavItem key={d.serial} active={selectedSerial === d.serial} onClick={() => onSelect({ device: d.serial })}>
              <span className={cx("size-1.5 shrink-0 rounded-full", online ? "bg-success" : "bg-outline-variant")} />
              <span className="min-w-0 flex-1 truncate">{name}</span>
              <span className="font-mono text-[10px] opacity-70">{d.transport === "Disconnected" ? "off" : d.transport}</span>
            </NavItem>
          );
        })}
      </div>

      <div className="p-4">
        <Button variant="filled" className="h-8 w-full" onClick={onAddDevice}>
          <Plus className="size-4" />
          Add device
        </Button>
      </div>
    </aside>
  );
}
