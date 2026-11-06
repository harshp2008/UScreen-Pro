import { Check } from "lucide-react";
import { useRef, useState } from "react";
import { Button, Segmented, Select, TextInput } from "../components/ui/controls";
import { PageHeader, QualityRow, Section, SettingRow, SliderRow, SwitchRow } from "../components/ui/layout";
import { ENCODERS, FPS, POSITIONS, STREAM_DETAIL } from "../lib/options";
import type { AppConfig } from "../lib/types";

interface Props {
  config: AppConfig;
  dirty: boolean;
  autostart: boolean;
  onChange: (patch: Partial<AppConfig>) => void;
  onSave: () => void;
  onAutostart: (enabled: boolean) => void;
}

const splitList = (s: string) =>
  s
    .split(",")
    .map((x) => x.trim())
    .filter(Boolean);

export function GlobalSettings({ config: c, dirty, autostart, onChange, onSave, onAutostart }: Props) {
  // The list itself is the stored state; the switch only reveals the field. Turning it off
  // stashes the list so switching back on restores it.
  const [appLock, setAppLock] = useState(c.locked_apps.length > 0);
  const stash = useRef<string[]>(c.locked_apps);
  function toggleAppLock(on: boolean) {
    setAppLock(on);
    if (on) onChange({ locked_apps: stash.current });
    else {
      stash.current = c.locked_apps;
      onChange({ locked_apps: [] });
    }
  }

  return (
    <div className="w-full space-y-2">
      <PageHeader title="Settings">
        <Button variant="filled" disabled={!dirty} onClick={onSave} icon={<Check className="size-4" />}>
          Save
        </Button>
      </PageHeader>

      <Section title="Video and encoding">
        <SettingRow label="Encoder" tip="Hardware or software codec used to compress the virtual display.">
          <Select label="Encoder" value={c.encoder} options={ENCODERS} onValueChange={(v) => onChange({ encoder: v })} />
        </SettingRow>
        <SettingRow label="Frame rate" tip="Target refresh rate of the virtual display.">
          <Select
            label="Frame rate"
            value={String(c.fps)}
            options={FPS}
            className="w-28"
            onValueChange={(v) => onChange({ fps: Number(v) })}
          />
        </SettingRow>
        <QualityRow quality={c.quality} onChange={(quality) => onChange({ quality })} />
        <SliderRow
          label="Bitrate ceiling"
          tip="Upper bound on stream bandwidth. Lower it on congested Wi-Fi."
          valueText={`${Math.round(c.bitrate / 1000)} Mbps`}
          value={c.bitrate}
          min={5000}
          max={60000}
          step={1000}
          onChange={(bitrate) => onChange({ bitrate })}
        />
        <SettingRow label="Stream detail" tip="Full sends every pixel; Half renders at half resolution for the lowest latency.">
          <Segmented
            label="Stream detail"
            value={String(c.stream_scale)}
            options={STREAM_DETAIL}
            onValueChange={(v) => onChange({ stream_scale: Number(v) })}
          />
        </SettingRow>
        <SwitchRow
          label="10-bit colour depth"
          tip="HEVC Main10. Removes gradient banding; the tablet must support 10-bit decoding."
          checked={c.ten_bit}
          onChange={(ten_bit) => onChange({ ten_bit })}
        />
        <SwitchRow
          label="Zero-latency HEVC tuning"
          tip="Disables B-frames and lookahead so frames leave the encoder immediately."
          checked={c.hevc_low_latency_tuning}
          onChange={(hevc_low_latency_tuning) => onChange({ hevc_low_latency_tuning })}
        />
      </Section>

      <Section title="Display and layout">
        <SettingRow label="Position" tip="Where the virtual display sits relative to your primary monitor.">
          <Segmented
            label="Position"
            value={c.position}
            options={POSITIONS}
            onValueChange={(position) => onChange({ position })}
          />
        </SettingRow>
        <SwitchRow
          label="Auto resolution"
          tip="Read the panel's native resolution from the tablet instead of using the fixed size below."
          checked={c.auto_resolution}
          onChange={(auto_resolution) => onChange({ auto_resolution })}
        />
        {!c.auto_resolution && (
          <SettingRow label="Fixed resolution" tip="Width and height in pixels.">
            <div className="flex items-center gap-2">
              <TextInput
                type="number"
                aria-label="Width"
                className="w-16 font-mono"
                value={c.width}
                onChange={(v) => onChange({ width: Number(v) })}
              />
              <span className="text-on-surface-variant">x</span>
              <TextInput
                type="number"
                aria-label="Height"
                className="w-16 font-mono"
                value={c.height}
                onChange={(v) => onChange({ height: Number(v) })}
              />
            </div>
          </SettingRow>
        )}
        <SwitchRow
          label="Keep-alive display"
          tip="Keep the virtual monitor and your window layout when the tablet disconnects."
          checked={c.keep_alive}
          onChange={(keep_alive) => onChange({ keep_alive })}
        />
      </Section>

      <Section title="Input and behaviour">
        <SwitchRow
          label="Stylus mode (graphics tablet)"
          tip="Use the tablet purely as a pen input device with no video stream."
          checked={c.pen_only}
          onChange={(pen_only) => onChange({ pen_only })}
        />
        <SwitchRow
          label="Auto-launch app"
          tip="Start the UScreen app on the tablet when it is plugged in."
          checked={c.auto_launch_app}
          onChange={(auto_launch_app) => onChange({ auto_launch_app })}
        />
        <SwitchRow
          label="Seamless pointer handoff"
          tip="Let the cursor cross from your monitor onto the tablet and back."
          checked={c.pointer_handoff}
          onChange={(pointer_handoff) => onChange({ pointer_handoff })}
        />
        <SwitchRow
          label="Max performance scheduling"
          tip="Raise process priority and tune sockets for latency. Uses more CPU."
          checked={c.max_performance_mode}
          onChange={(max_performance_mode) => onChange({ max_performance_mode })}
        />
        <SwitchRow
          label="Enable app locking (Mutter)"
          tip="Keep chosen applications on the virtual display; other windows are moved off it. Off by default."
          checked={appLock}
          onChange={toggleAppLock}
        />
        {appLock && (
          <SettingRow label="Locked applications" tip="Comma-separated application IDs, e.g. org.gnome.Nautilus.">
            <TextInput
              aria-label="Locked applications"
              className="w-64 font-mono"
              placeholder="org.gnome.Nautilus, gimp"
              value={c.locked_apps.join(", ")}
              onChange={(v) => onChange({ locked_apps: splitList(v) })}
            />
          </SettingRow>
        )}
      </Section>

      <Section title="Connection and security">
        <SwitchRow
          label="Auto-enable Wi-Fi ADB"
          tip="Switch the tablet to wireless debugging as soon as it is plugged in over USB."
          checked={c.auto_enable_wifi_adb}
          onChange={(auto_enable_wifi_adb) => onChange({ auto_enable_wifi_adb })}
        />
        <SwitchRow
          label="Allow legacy fallback"
          tip="Use per-device tcpip ports for Android 10 and older, which lack wireless pairing."
          checked={c.allow_legacy_fallback}
          onChange={(allow_legacy_fallback) => onChange({ allow_legacy_fallback })}
        />
        <SwitchRow
          label="Require session security token"
          tip="Reject local connections to the streaming ports unless they present the per-session token."
          checked={c.require_token}
          onChange={(require_token) => onChange({ require_token })}
        />
      </Section>

      <Section title="System">
        <SwitchRow
          label="Start with desktop"
          tip="Enable the uscreen.service systemd user unit. Applies immediately."
          checked={autostart}
          onChange={onAutostart}
        />
        <SwitchRow
          label="Check for updates"
          tip="Compare host and tablet app versions at launch."
          checked={c.check_updates}
          onChange={(check_updates) => onChange({ check_updates })}
        />
      </Section>
    </div>
  );
}
