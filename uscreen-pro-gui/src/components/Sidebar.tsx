import { Activity, FlaskConical, Plus, Play, Sliders, Square, Tablet, WifiOff } from "lucide-react";
import {
  NavigationDrawer,
  NavigationDrawerContent,
  NavigationDrawerItem,
  NavigationDrawerSectionHeader,
} from "@bug-on/m3-expressive/navigation";
import type { AppConfig, ConnectedDevice, View } from "../lib/types";
import { Button } from "./ui/controls";

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

  /*
   * One primary call-to-action replaces the old status card. Stopped: Primary (Emerald) "Start daemon".
   * Running: Error (red) "Stop daemon". The label and colour are the status; PID is the tooltip.
   */
  const daemonButton = (
    <Button
      variant={daemonRunning ? "error" : "filled"}
      fullWidth
      loading={daemonBusy !== null}
      disabled={daemonBusy !== null}
      title={daemonBusy ? busyLabel : daemonRunning ? `Running, PID ${daemonPid ?? "?"}` : "Stopped"}
      icon={daemonRunning ? <Square className="size-[18px]" /> : <Play className="size-[18px]" />}
      onClick={onToggleDaemon}
    >
      {daemonRunning ? "Stop Daemon" : "Start Daemon"}
    </Button>
  );

  return (
    <NavigationDrawer
      variant="standard"
      width={240}
      aria-label="UScreen navigation"
      footer={
        <Button variant="tonal" fullWidth icon={<Plus className="size-[18px]" />} onClick={onAddDevice}>
          Add device
        </Button>
      }
      className="h-auto min-h-0 shrink-0 gap-2 rounded-none border-r border-outline-variant bg-transparent px-3 pb-3 pt-3"
    >
      <NavigationDrawerContent>
        {daemonButton}
        {/* Secondary-container indicator with On-Secondary-Container label is the drawer item's active state. */}
        <NavigationDrawerItem
          className="h-9"
          selected={view === "settings"}
          icon={<Sliders className="size-5" />}
          label="Settings"
          onClick={() => onSelect("settings")}
        />
        <NavigationDrawerItem
          className="h-9"
          selected={view === "advanced"}
          icon={<FlaskConical className="size-5" />}
          label="Advanced"
          onClick={() => onSelect("advanced")}
        />
        <NavigationDrawerItem
          className="h-9"
          selected={view === "health"}
          icon={<Activity className="size-5" />}
          label="System health"
          onClick={() => onSelect("health")}
        />

        <NavigationDrawerSectionHeader>Devices ({devices.length})</NavigationDrawerSectionHeader>
        {devices.length === 0 && <div className="px-4 py-2 text-on-surface-variant">No devices yet</div>}
        {devices.map((d) => {
          const name = config?.tablets[d.serial]?.nickname?.trim() || d.model || d.serial;
          const online = d.transport !== "Disconnected" && d.state === "device";
          return (
            <NavigationDrawerItem
              className="h-9"
              key={d.serial}
              selected={selectedSerial === d.serial}
              icon={online ? <Tablet className="size-5" /> : <WifiOff className="size-5" />}
              label={name}
              badge={d.transport === "Disconnected" ? "off" : d.transport}
              onClick={() => onSelect({ device: d.serial })}
            />
          );
        })}
      </NavigationDrawerContent>
    </NavigationDrawer>
  );
}
