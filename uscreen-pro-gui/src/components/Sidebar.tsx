import { Activity, Plus, Play, Sliders, Square, Tablet, WifiOff } from "lucide-react";
import {
  NavigationDrawer,
  NavigationDrawerContent,
  NavigationDrawerItem,
  NavigationDrawerSectionHeader,
} from "@bug-on/m3-expressive/navigation";
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

  /* Status row + button share the sidebar's single content column, so every button in the
     sidebar spans the same left and right edges. */
  const daemonBlock = (
    <div className="space-y-2 pb-2">
      <div className="flex items-center gap-2 px-1">
        <span
          className={cx("size-2.5 shrink-0 rounded-full", daemonRunning ? "bg-success" : "bg-error")}
          aria-hidden
        />
        <div className="min-w-0">
          <div className="text-xs font-semibold text-on-surface">Host daemon</div>
          <div className="truncate text-xs text-on-surface-variant">
            {daemonBusy ? busyLabel : daemonRunning ? `Running, PID ${daemonPid ?? "?"}` : "Stopped"}
          </div>
        </div>
      </div>
      {/* Filled = Primary container + On-Primary label; pill that springs to a rounded square on press. */}
      <Button
        variant={daemonRunning ? "tonal" : "filled"}
        fullWidth
        loading={daemonBusy !== null}
        disabled={daemonBusy !== null}
        icon={daemonRunning ? <Square className="size-[18px]" /> : <Play className="size-[18px]" />}
        onClick={onToggleDaemon}
      >
        {daemonRunning ? "Stop daemon" : "Start daemon"}
      </Button>
    </div>
  );

  return (
    <NavigationDrawer
      variant="standard"
      width={240}
      aria-label="UScreen navigation"
      footer={
        <Button
          variant="tonal"
          fullWidth
          icon={<Plus className="size-[18px]" />}
          onClick={onAddDevice}
        >
          Add device
        </Button>
      }
      className="!h-auto min-h-0 shrink-0 gap-2 border-r border-outline-variant !bg-transparent !rounded-none px-4 pb-4 pt-2"
    >
      <NavigationDrawerContent>
        {daemonBlock}
        {/* Secondary-container indicator with On-Secondary-Container label is the drawer item's active state. */}
        <NavigationDrawerItem
          className="!h-9"
          selected={view === "settings"}
          icon={<Sliders className="size-5" />}
          label="Settings"
          onClick={() => onSelect("settings")}
        />
        <NavigationDrawerItem
          className="!h-9"
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
          className="!h-9"
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
