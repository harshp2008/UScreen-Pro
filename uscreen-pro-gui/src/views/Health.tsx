import type { ReactNode } from "react";
import { Button } from "../components/ui/controls";
import { PageHeader, Section } from "../components/ui/layout";
import type { DaemonStatus } from "../lib/types";

function Dot({ ok }: { ok: boolean }) {
  return <span className={`size-2 shrink-0 rounded-full ${ok ? "bg-success" : "bg-error"}`} />;
}

function Card({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0 rounded-lg border border-outline-variant bg-card p-3">
      <div className="text-[11px] text-on-surface-variant">{label}</div>
      <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 break-all text-[13px] font-medium text-on-surface">
        {children}
      </div>
    </div>
  );
}

export function Health({ status, onRestartAdb }: { status: DaemonStatus; onRestartAdb: () => void }) {
  return (
    <div className="w-full space-y-2">
      <PageHeader title="System health">
        <Button onClick={onRestartAdb}>Restart ADB</Button>
      </PageHeader>

      <div className="grid grid-cols-[repeat(auto-fit,minmax(280px,1fr))] gap-3">
        {/* One card for daemon + control channel; the sidebar owns the start/stop action. */}
        <Card label="Daemon">
          <Dot ok={status.is_running && status.control_connected} />
          {status.is_running
            ? `Running, PID ${status.pid}, control ${status.control_connected ? "connected" : "offline"}`
            : "Not running"}
          <span className="w-full font-mono text-xs font-normal text-on-surface-variant">{status.control_socket}</span>
        </Card>
        <Card label="Build">
          <span className="font-mono">{status.host_version}</span>
          {status.is_dirty && (
            <span className="rounded-full bg-tertiary-container px-2 py-0.5 text-[11px] font-normal text-on-tertiary-container">dirty</span>
          )}
        </Card>
      </div>

      <Section title="Devices">
        <div className="overflow-x-auto">
          <table className="w-full min-w-[480px] text-left">
            <thead>
              <tr className="text-[11px] text-on-surface-variant">
                {["Model", "Serial", "Link", "State", "App"].map((h) => (
                  <th key={h} className="whitespace-nowrap px-3 py-2 font-medium">
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody className="divide-y divide-outline-variant/60">
              {status.devices.map((d) => (
                <tr key={d.serial}>
                  <td className="whitespace-nowrap px-3 py-2 text-on-surface">{d.model}</td>
                  <td className="whitespace-nowrap px-3 py-2 font-mono text-on-surface-variant">{d.serial}</td>
                  <td className="whitespace-nowrap px-3 py-2 text-on-surface-variant">{d.transport}</td>
                  <td className={`whitespace-nowrap px-3 py-2 ${d.state === "device" ? "text-success" : "text-warning"}`}>
                    {d.state}
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 font-mono text-on-surface-variant">
                    {d.device_version ?? "-"}
                  </td>
                </tr>
              ))}
              {status.devices.length === 0 && (
                <tr>
                  <td colSpan={5} className="px-3 py-4 text-center text-on-surface-variant">
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
