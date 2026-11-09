import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Segmented, Select, TextInput } from "../components/ui/controls";
import { PageHeader, QualityRow, Section, SettingRow, SliderRow, SwitchRow } from "../components/ui/layout";
import { ENCODERS, FPS, IDLE_HIDE, PEN_MENU_HOLD, PEN_MENU_STYLE, POSITIONS, STREAM_DETAIL } from "../lib/options";
import { PowerSettings } from "../components/PowerSettings";
import type { AppConfig, CurvePreset } from "../lib/types";

interface Props {
  config: AppConfig;
  dirty: boolean;
  autostart: boolean;
  onChange: (patch: Partial<AppConfig>) => void;
  onSave: () => void;
  onAutostart: (enabled: boolean) => void;
  /** Persist the saved fade curves immediately. */
  onPresetsChange: (presets: CurvePreset[]) => void;
}

/** Settings page for the defaults every device inherits, plus the daemon autostart switch. */
export function GlobalSettings({ config: c, dirty, autostart, onChange, onSave, onAutostart, onPresetsChange }: Props) {
  return (
    <div className="w-full space-y-2">
      <PageHeader title="Settings">
        <Button size="sm" disabled={!dirty} onClick={onSave}><Check />
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
          formatValue={(v) => `${Math.round(v / 1000)} Mbps`}
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
        {/* Only hevc_vaapi has these tuning flags (NVENC always runs zero-latency, H.264 has none). */}
        {c.encoder === "hevc_vaapi" && (
          <SwitchRow
            label="Zero-latency HEVC tuning"
            tip="Disables B-frames and lookahead so frames leave the encoder immediately."
            checked={c.hevc_low_latency_tuning}
            onChange={(hevc_low_latency_tuning) => onChange({ hevc_low_latency_tuning })}
          />
        )}
      </Section>

      <Section title="Display and layout">
        <SwitchRow
          label="Use Software Mouse Cursor (Slower, more compatible)"
          tip="Off (default): the fast path, with the 1x1 boundary window that clears the ghost cursor on GNOME/Xwayland. On: plain composited cursor with no compositor-specific helper; use it on KDE, Sway or anything where the cursor misbehaves."
          checked={c.software_cursor}
          onChange={(software_cursor) => onChange({ software_cursor })}
        />
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
              <span className="text-muted-foreground">x</span>
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
      </Section>

      <Section title="Pen and drawing tablets">
        <SwitchRow
          label="Physical tablet support"
          tip="Use a plugged-in drawing tablet (XP-Pen, Wacom, Huion, ...) with pressure, tilt and a visible cursor. UScreen takes the tablet exclusively while this is on. Off by default; not needed on X11."
          checked={c.physical_tablet_support}
          onChange={(physical_tablet_support) => onChange({ physical_tablet_support })}
        />
        <SwitchRow
          label="Pen menu"
          tip="Hold the pen still on the screen to get a small menu: cut, copy, paste, select all, undo. Works on GNOME (Wayland) with the UScreen extension; never shown in drawing apps."
          checked={c.pen_menu}
          onChange={(pen_menu) => onChange({ pen_menu })}
        />
        {c.pen_menu && (
          <>
            <SettingRow
              label="Hold time for the menu"
              tip="How long to hold the pen still before the menu appears."
            >
              <Select
                label="Hold time for the menu"
                value={String(c.pen_menu_hold_ms)}
                options={PEN_MENU_HOLD}
                className="w-36"
                onValueChange={(v) => onChange({ pen_menu_hold_ms: Number(v) })}
              />
            </SettingRow>
            <SettingRow label="Menu buttons" tip="Show the menu buttons as text labels or as icons.">
              <Select
                label="Menu buttons"
                value={c.pen_menu_style}
                options={PEN_MENU_STYLE}
                className="w-36"
                onValueChange={(v) => onChange({ pen_menu_style: v })}
              />
            </SettingRow>
          </>
        )}
        <SettingRow
          label="Hide idle pen cursor"
          tip="Hide the cursor when the pen has been completely out of range for this long. Never keeps it visible like a mouse cursor. Works on GNOME (Wayland)."
        >
          <Select
            label="Hide idle pen cursor"
            value={String(c.stylus_idle_hide_timeout)}
            options={IDLE_HIDE}
            className="w-36"
            onValueChange={(v) => onChange({ stylus_idle_hide_timeout: Number(v) })}
          />
        </SettingRow>
      </Section>

      <PowerSettings
        values={c}
        onChange={onChange}
        presets={c.curve_presets}
        onPresetsChange={onPresetsChange}
      />

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
