import { Download, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { PageHeader, Section } from "../components/ui/layout";
import type { DaemonStatus } from "../lib/types";

function Dot({ ok }: { ok: boolean }) {
  return <span className={`size-2 shrink-0 rounded-full ${ok ? "bg-success" : "bg-destructive"}`} />;
}

function Card({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0 rounded-lg border border-border bg-card p-3">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 break-all text-[13px] font-medium text-foreground">
        {children}
      </div>
    </div>
  );
}

export function Health({
  status,
  onRestartAdb,
  onInstallApk,
}: {
  status: DaemonStatus;
  onRestartAdb: () => void;
  onInstallApk: (serial: string) => void;
}) {
  const tools = status.devices.filter((d) => d.state === "device");
  return (
    <div className="w-full space-y-2">
      <PageHeader title="System health">
        <Button size="sm" variant="secondary" onClick={onRestartAdb}>Restart ADB</Button>
      </PageHeader>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-3">
        {/* One card for daemon + control channel; the sidebar owns the start/stop action. */}
        <Card label="Daemon">
          <Dot ok={status.is_running && status.control_connected} />
          {status.is_running
            ? `Running, PID ${status.pid}, control ${status.control_connected ? "connected" : "offline"}`
            : "Not running"}
          <span className="w-full font-mono text-xs font-normal text-muted-foreground">{status.control_socket}</span>
        </Card>
        <Card label="Build">
          <span className="font-mono">{status.host_version}</span>
          {status.is_dirty && (
            <Badge variant="secondary">dirty</Badge>
          )}
        </Card>
      </div>

      <Section title="Device tools" hint="Act on a connected device over ADB">
        {tools.map((d) => {
          const mismatch = !!d.device_version && d.device_version !== status.host_version;
          return (
            <div key={d.serial} className="flex min-h-12 flex-wrap items-center justify-between gap-3 px-3 py-2">
              <div className="min-w-0">
                <div className="truncate text-foreground">{d.model}</div>
                <div className="font-mono text-[11px] text-muted-foreground">
                  {d.serial} · app {d.device_version ?? "not installed"}
                </div>
              </div>
              <div className="flex items-center gap-2">
                <Button size="sm" variant="secondary" onClick={() => onInstallApk(d.serial)}>
                  <RefreshCw />
                  Force Reinstall APK
                </Button>
                <Button size="sm" variant="secondary" onClick={() => onInstallApk(d.serial)} disabled={!mismatch}>
                  <Download />
                  Update APK
                </Button>
              </div>
            </div>
          );
        })}
        {tools.length === 0 && (
          <div className="px-3 py-4 text-center text-muted-foreground">No authorized device connected.</div>
        )}
      </Section>

      <Section title="Devices">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[480px] text-left">
            <thead>
              <tr className="text-[11px] text-muted-foreground">
                {["Model", "Serial", "Link", "State", "App"].map((h) => (
                  <th key={h} className="whitespace-nowrap px-3 py-2 font-medium">
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {status.devices.map((d) => (
                <tr key={d.serial}>
                  <td className="whitespace-nowrap px-3 py-2 text-foreground">{d.model}</td>
                  <td className="whitespace-nowrap px-3 py-2 font-mono text-muted-foreground">{d.serial}</td>
                  <td className="whitespace-nowrap px-3 py-2 text-muted-foreground">{d.transport}</td>
                  <td className={`whitespace-nowrap px-3 py-2 ${d.state === "device" ? "text-success" : "text-warning"}`}>
                    {d.state}
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 font-mono text-muted-foreground">
                    {d.device_version ?? "-"}
                  </td>
                </tr>
              ))}
              {status.devices.length === 0 && (
                <tr>
                  <td colSpan={5} className="px-3 py-4 text-center text-muted-foreground">
                    No devices known.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </Section>
    </div>
  );
}
