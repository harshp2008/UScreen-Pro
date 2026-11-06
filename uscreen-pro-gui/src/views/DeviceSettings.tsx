import { AlertTriangle, Check, Download, Pause, Play, Trash2 } from "lucide-react";
import { AppLockList } from "../components/ui/AppLockList";
import { Button } from "@/components/ui/button";
import { Segmented, Select, TextInput } from "../components/ui/controls";
import { PageHeader, QualityRow, Section, SettingRow, SliderRow, SwitchRow, type Override } from "../components/ui/layout";
import { ENCODERS, FPS, POSITIONS, STREAM_DETAIL, TRANSPORTS, labelOf, qualityLabel } from "../lib/options";
import type { AppConfig, ConnectedDevice, TabletProfile } from "../lib/types";

interface Props {
  serial: string;
  device?: ConnectedDevice;
  profile: TabletProfile;
  config: AppConfig;
  hostVersion?: string;
  onChange: (patch: Partial<TabletProfile>) => void;
  onSave: () => void;
  onDelete: () => void;
  onTogglePause: () => void;
  onInstallApk: () => void;
}

export function DeviceSettings({
  serial,
  device,
  profile: p,
  config: g,
  hostVersion,
  onChange,
  onSave,
  onDelete,
  onTogglePause,
  onInstallApk,
}: Props) {
  /** Build the override toggle for a key: on seeds the global value, off clears the override. */
  function ov<K extends keyof TabletProfile>(key: K, seed: TabletProfile[K], inherited: string): Override {
    return {
      enabled: p[key] !== undefined,
      onChange: (on) => onChange({ [key]: on ? seed : undefined } as Partial<TabletProfile>),
      inherited: `Global: ${inherited}`,
    };
  }

  const online = device?.state === "device";
  const mismatch = device?.device_version && hostVersion && device.device_version !== hostVersion;

  return (
    <div className="w-full space-y-2">
      <PageHeader
        title={p.nickname?.trim() || device?.model || serial}
        subtitle={`${serial}  ${device?.transport ?? "Saved"}`}
      >
        {online && (
          <Button size="sm" variant="secondary" onClick={onTogglePause}>
            {device?.paused ? <Play /> : <Pause />}
            {device?.paused ? "Resume" : "Pause"}
          </Button>
        )}
        <Button size="sm" variant="secondary" onClick={onInstallApk} disabled={!online}><Download />
          Push APK
        </Button>
        <Button size="sm" onClick={onSave}><Check />
          Save
        </Button>
      </PageHeader>

      {mismatch && (
        <div className="flex items-center justify-between gap-3 rounded-lg bg-warning/15 px-3 py-2 text-warning">
          <span className="flex items-center gap-2">
            <AlertTriangle className="size-4 shrink-0" />
            Tablet app is {device?.device_version}, host is {hostVersion}.
          </span>
          <Button size="sm" variant="ghost" className="text-warning" onClick={onInstallApk}>
            Update app
          </Button>
        </div>
      )}

      <Section title="Identity">
        <SettingRow label="Nickname" tip="Shown in the sidebar instead of the model name.">
          <TextInput
            aria-label="Nickname"
            placeholder={device?.model || serial}
            value={p.nickname ?? ""}
            onChange={(v) => onChange({ nickname: v || undefined })}
          />
        </SettingRow>
        <SettingRow label="Preferred transport" tip="Which link to use when both USB and Wi-Fi are available.">
          <Select
            label="Preferred transport"
            value={p.preferred_transport ?? "auto"}
            options={TRANSPORTS}
            className="w-60"
            onValueChange={(preferred_transport) => onChange({ preferred_transport })}
          />
        </SettingRow>
        <SettingRow label="Wi-Fi address" tip="IP:port used for wireless streaming to this tablet.">
          <TextInput
            aria-label="Wi-Fi address"
            className="w-52 font-mono"
            placeholder="192.168.1.50:5555"
            value={p.wifi_address ?? ""}
            onChange={(v) => onChange({ wifi_address: v || undefined })}
          />
        </SettingRow>
      </Section>

      <Section title="Overrides" hint="Switch on to override the global value">
        <SettingRow
          label="Encoder"
          tip="Codec for this tablet only."
          override={ov("encoder", g.encoder, labelOf(ENCODERS, g.encoder))}
        >
          <Select
            label="Encoder"
            value={p.encoder ?? g.encoder}
            options={ENCODERS}
            onValueChange={(encoder) => onChange({ encoder })}
          />
        </SettingRow>
        <SettingRow label="Frame rate" override={ov("fps", g.fps, `${g.fps} fps`)}>
          <Select
            label="Frame rate"
            value={String(p.fps ?? g.fps)}
            options={FPS}
            className="w-28"
            onValueChange={(v) => onChange({ fps: Number(v) })}
          />
        </SettingRow>
        <QualityRow
          quality={p.quality ?? g.quality}
          onChange={(quality) => onChange({ quality })}
          override={ov("quality", g.quality, `${g.quality} — ${qualityLabel(g.quality)}`)}
        />
        <SliderRow
          label="Bitrate ceiling"
          formatValue={(v) => `${Math.round(v / 1000)} Mbps`}
          value={p.bitrate ?? g.bitrate}
          min={5000}
          max={60000}
          step={1000}
          onChange={(bitrate) => onChange({ bitrate })}
          override={ov("bitrate", g.bitrate, `${Math.round(g.bitrate / 1000)} Mbps`)}
        />
        <SwitchRow
          label="10-bit colour depth"
          checked={p.ten_bit ?? g.ten_bit}
          onChange={(ten_bit) => onChange({ ten_bit })}
          override={ov("ten_bit", g.ten_bit, g.ten_bit ? "10-bit" : "8-bit")}
        />
        <SettingRow
          label="Stream detail"
          override={ov("stream_scale", g.stream_scale, labelOf(STREAM_DETAIL, g.stream_scale))}
        >
          <Segmented
            label="Stream detail"
            value={String(p.stream_scale ?? g.stream_scale)}
            options={STREAM_DETAIL}
            onValueChange={(v) => onChange({ stream_scale: Number(v) })}
          />
        </SettingRow>
        <SettingRow
          label="Position"
          override={ov("position", g.position, labelOf(POSITIONS, g.position))}
        >
          <Segmented
            label="Position"
            value={p.position ?? g.position}
            options={POSITIONS}
            onValueChange={(position) => onChange({ position })}
          />
        </SettingRow>
        <SwitchRow
          label="Keep-alive display"
          checked={p.keep_alive ?? g.keep_alive}
          onChange={(keep_alive) => onChange({ keep_alive })}
          override={ov("keep_alive", g.keep_alive, g.keep_alive ? "On" : "Off")}
        />
        <SettingRow
          label="App locking"
          tip="Application IDs pinned to this device, instead of the global list in Advanced."
          override={ov("locked_apps", g.locked_apps, g.locked_apps.join(", ") || "none")}
        >
          <div className="w-80">
            <AppLockList
              apps={p.locked_apps ?? g.locked_apps}
              onChange={(locked_apps) => onChange({ locked_apps })}
            />
          </div>
        </SettingRow>
        <SettingRow label="Resolution" tip="Leave at 0 x 0 to use the panel's own resolution.">
          <div className="flex items-center gap-2">
            <TextInput
              type="number"
              aria-label="Width"
              className="w-16 font-mono"
              value={p.width ?? 0}
              onChange={(v) => onChange({ width: Number(v) })}
            />
            <span className="text-muted-foreground">x</span>
            <TextInput
              type="number"
              aria-label="Height"
              className="w-16 font-mono"
              value={p.height ?? 0}
              onChange={(v) => onChange({ height: Number(v) })}
            />
          </div>
        </SettingRow>
      </Section>

      {device?.transport === "Disconnected" && (
        <div className="flex justify-end">
          <Button size="sm" variant="destructive" onClick={onDelete}><Trash2 />
            Forget device
          </Button>
        </div>
      )}
    </div>
  );
}
