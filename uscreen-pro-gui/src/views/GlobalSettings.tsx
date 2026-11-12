/*
 * The global Settings page: the defaults every device inherits. Presentational; state arrives as props from App.tsx.
 */
import { useState } from "react";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Segmented, Select, TextInput } from "../components/ui/controls";
import { PageHeader, QualityRow, Section, SettingRow, SliderRow, SwitchRow, Tabs } from "../components/ui/layout";
import { AUTO_OPEN, ENCODERS, INPUT_MODES, FPS, POSITIONS, STREAM_DETAIL, modeLabel } from "../lib/options";
import { ModePanel, ModeSection, TabletPanel } from "../components/PowerSettings";
import type { AppConfig, CurvePreset, InputMode } from "../lib/types";

/**
 * Props of the Settings page: global settings, whether they have unsaved edits, autostart state and callbacks.
 */
interface Props {
  config: AppConfig;
  /** An edit that restarts the stream is waiting for Apply. */
  dirty: boolean;
  /** Autosave state, shown beside the Apply button. */
  saveNote?: "" | "saving" | "saved";
  autostart: boolean;
  onChange: (patch: Partial<AppConfig>) => void;
  onSave: () => void;
  onAutostart: (enabled: boolean) => void;
  /** Persist the saved fade curves immediately. */
  onPresetsChange: (presets: CurvePreset[]) => void;
}

/** Settings page for the defaults every device inherits, plus the daemon autostart switch. */
export function GlobalSettings({ config: c, dirty, saveNote = "", autostart, onChange, onSave, onAutostart, onPresetsChange }: Props) {
  /* The tab and the mode being viewed are only view state: clicking them never changes a setting. */
  const [tab, setTab] = useState("general");
  const [viewMode, setViewMode] = useState<InputMode>(c.input_mode);
  const modeOptions = INPUT_MODES.map((o) => {
    const name = modeLabel(o.value, c.stylus_style);
    return { ...o, label: o.value === c.input_mode ? `${name} · Default` : name };
  });
  const values = c;

  return (
    <div className="w-full space-y-2">
      <PageHeader title="Settings">
        {saveNote && <span className="text-xs text-muted-foreground">{saveNote === "saving" ? "Saving…" : "Saved"}</span>}
        {dirty && (
          <Button size="sm" onClick={onSave}><Check />
            Apply
          </Button>
        )}
      </PageHeader>

      <Tabs
        label="Settings category"
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "general", label: "General" },
          { value: "modes", label: "Modes" },
          { value: "tablet", label: "Android app" },
          { value: "connection", label: "Connection" },
        ]}
      />

      {tab === "general" && (
        <>
          <ModeSection values={values} onChange={onChange} title="Default mode" label="Default mode" />
          <Section title="General">
          <SettingRow label="Open app on connect" tip="When the computer opens UScreen on the tablet by itself.">
            <Select
              label="Open app on connect"
              value={c.auto_open_app}
              options={AUTO_OPEN}
              className="w-56"
              onValueChange={(auto_open_app) => onChange({ auto_open_app: auto_open_app as AppConfig["auto_open_app"] })}
            />
          </SettingRow>
          <SwitchRow
            label="Max performance scheduling"
            tip="Raise process priority and tune sockets for latency. Uses more CPU."
            checked={c.max_performance_mode}
            onChange={(max_performance_mode) => onChange({ max_performance_mode })}
          />
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
        </>
      )}

      {tab === "modes" && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-3 pt-3">
            <Segmented
              label="Mode to edit"
              value={viewMode}
              options={modeOptions}
              onValueChange={(m) => setViewMode(m as InputMode)}
            />
            {viewMode !== c.input_mode && (
              <Button size="sm" variant="secondary" onClick={() => onChange({ input_mode: viewMode })}>
                Make default
              </Button>
            )}
          </div>
          <ModePanel
            values={values}
            onChange={onChange}
            mode={viewMode}
            displayExtra={
              <>
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
            <SettingRow label="Stream detail" tip="Full sends every pixel. Half renders at half size for lower latency.">
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
              tip="Draw the cursor in software. Slower; use it on KDE, Sway or if the cursor misbehaves."
              checked={c.software_cursor}
              onChange={(software_cursor) => onChange({ software_cursor })}
            />
            <SettingRow
              label="Interface size"
              tip="Scale of text and windows on the tablet's screen, like GNOME's display scale. Automatic lets GNOME choose."
            >
              <Select
                label="Interface size mode"
                value={c.interface_scale === 0 ? "auto" : "custom"}
                options={[
                  { value: "auto", label: "Automatic" },
                  { value: "custom", label: "Custom" },
                ]}
                className="w-36"
                onValueChange={(v) => onChange({ interface_scale: v === "auto" ? 0 : 100 })}
              />
            </SettingRow>
            {c.interface_scale > 0 && (
              <SliderRow
                label="Scale"
                tip="Smaller is faster for whole-desktop screenshots. Snaps to a scale the display supports."
                formatValue={(n) => `${Math.round(n)}%`}
                minLabel="50%"
                maxLabel="300%"
                value={c.interface_scale}
                min={50}
                max={300}
                step={5}
                onChange={(interface_scale) => onChange({ interface_scale })}
              />
            )}
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
            <SwitchRow
              label="Seamless pointer handoff"
              tip="Let the cursor cross from your monitor onto the tablet and back."
              checked={c.pointer_handoff}
              onChange={(pointer_handoff) => onChange({ pointer_handoff })}
            />
          </Section>
              </>
            }
            stylusExtra={
              <Section title="Pen and drawing tablets">
                <SwitchRow
                  label="Physical tablet support"
                  tip="Use a plugged-in drawing tablet (Wacom, XP-Pen, Huion). UScreen takes it exclusively."
                  checked={c.physical_tablet_support}
                  onChange={(physical_tablet_support) => onChange({ physical_tablet_support })}
                />
              </Section>
            }
          />
        </>
      )}

      {tab === "tablet" && (
        <TabletPanel
          values={values}
          onChange={onChange}
          presets={c.curve_presets}
          onPresetsChange={onPresetsChange}
        />
      )}

      {tab === "connection" && (
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
      )}
    </div>
  );
}
