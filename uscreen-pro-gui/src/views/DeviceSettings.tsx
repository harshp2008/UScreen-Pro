import { useState } from "react";
import { AlertTriangle, Check, Pause, Play, Trash2, Wifi } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { AppLockList } from "../components/ui/AppLockList";
import { Button } from "@/components/ui/button";
import { Segmented, Select, TextInput } from "../components/ui/controls";
import { PageHeader, QualityRow, Tabs, Section, SettingRow, SliderRow, SwitchRow, type Override } from "../components/ui/layout";
import { ENCODERS, INPUT_MODES, FPS, POSITIONS, STREAM_DETAIL, TRANSPORTS, labelOf, qualityLabel } from "../lib/options";
import { ModePanel, ModeSection, TabletPanel, POWER_TIP, type PowerValues } from "../components/PowerSettings";
import type { AppConfig, ConnectedDevice, CurvePreset, TabletProfile } from "../lib/types";

interface Props {
  serial: string;
  device?: ConnectedDevice;
  profile: TabletProfile;
  config: AppConfig;
  hostVersion?: string;
  onChange: (patch: Partial<TabletProfile>) => void;
  /** Persist the saved fade curves immediately (they belong to the global config). */
  onPresetsChange: (presets: CurvePreset[]) => void;
  onSave: () => void;
  onDelete: () => void;
  onTogglePause: () => void;
  onInstallApk: () => void;
  /** Re-open the Wireless Setup step for this device. */
  onConfigureWireless: () => void;
  /** True when the profile differs from what is saved. */
  dirty: boolean;
}

/** Settings page of one paired device: stream overrides, transport, pause/update/wireless actions and Save for that device only. */
export function DeviceSettings({
  serial,
  device,
  profile: p,
  config: g,
  hostVersion,
  onChange,
  onPresetsChange,
  onSave,
  onDelete,
  onTogglePause,
  onInstallApk,
  onConfigureWireless,
  dirty,
}: Props) {
  const [confirmForget, setConfirmForget] = useState(false);
  /* Which tab is showing; view state only. */
  const [tab, setTab] = useState("mode");
  /** Build the override toggle for a key: on seeds the global value, off clears the override. */
  function ov<K extends keyof TabletProfile>(key: K, seed: TabletProfile[K], inherited: string): Override {
    return {
      enabled: p[key] !== undefined,
      onChange: (on) => onChange({ [key]: on ? seed : undefined } as Partial<TabletProfile>),
      inherited: `Global: ${inherited}`,
    };
  }

  /** Effective power values: the device's override where set, the global value otherwise. */
  const power: PowerValues = {
    input_mode: p.input_mode ?? g.input_mode,
    touch_input: p.touch_input ?? g.touch_input,
    blank_pen_only: p.blank_pen_only ?? g.blank_pen_only,
    immersive_mode: p.immersive_mode ?? g.immersive_mode,
    show_stats: p.show_stats ?? g.show_stats,
    orientation: p.orientation ?? g.orientation,
    theme_mode: p.theme_mode ?? g.theme_mode,
    keep_connected_in_background: p.keep_connected_in_background ?? g.keep_connected_in_background,
    keep_display_alive_when_locked: p.keep_display_alive_when_locked ?? g.keep_display_alive_when_locked,
    touch_on_lock_screen: p.touch_on_lock_screen ?? g.touch_on_lock_screen,
    battery_saver_on_disconnect: p.battery_saver_on_disconnect ?? g.battery_saver_on_disconnect,
    battery_saver_lock: p.battery_saver_lock ?? g.battery_saver_lock,
    idle_dim_enabled: p.idle_dim_enabled ?? g.idle_dim_enabled,
    idle_dim_timeout_secs: p.idle_dim_timeout_secs ?? g.idle_dim_timeout_secs,
    dim_curve: p.dim_curve ?? g.dim_curve,
    undim_curve: p.undim_curve ?? g.undim_curve,
    dim_duration_ms: p.dim_duration_ms ?? g.dim_duration_ms,
    undim_duration_ms: p.undim_duration_ms ?? g.undim_duration_ms,
    touchpad_haptics: p.touchpad_haptics ?? g.touchpad_haptics,
    native_gestures: p.native_gestures ?? g.native_gestures,
    touchpad_sensitivity: p.touchpad_sensitivity ?? g.touchpad_sensitivity,
    right_click_method: p.right_click_method ?? g.right_click_method,
    touchpad_shape: p.touchpad_shape ?? g.touchpad_shape,
    stylus_style: p.stylus_style ?? g.stylus_style,
    stylus_button_primary: p.stylus_button_primary ?? g.stylus_button_primary,
    stylus_button_secondary: p.stylus_button_secondary ?? g.stylus_button_secondary,
    pen_menu: p.pen_menu ?? g.pen_menu,
    pen_menu_style: p.pen_menu_style ?? g.pen_menu_style,
    pen_menu_hold_ms: p.pen_menu_hold_ms ?? g.pen_menu_hold_ms,
    stylus_idle_hide_timeout: p.stylus_idle_hide_timeout ?? g.stylus_idle_hide_timeout,
  };

  const powerOverride = (key: keyof PowerValues, inherited: string) => ov(key, g[key], inherited);

  const online = device?.state === "device";
  /* Only an explicit false (wireless skipped in the wizard) locks the Wi-Fi controls; older profiles stay editable. */
  const wirelessOff = p.wireless_configured === false;
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
        <Button size="sm" onClick={onSave} disabled={!dirty}><Check />
          Save
        </Button>
      </PageHeader>

      {mismatch && (
        <div className="flex items-center justify-between gap-3 rounded-lg bg-warning/15 px-3 py-2 text-warning">
          <span className="flex items-center gap-2">
            <AlertTriangle className="size-4 shrink-0" />
            Android app is {device?.device_version}, host is {hostVersion}.
          </span>
          <Button size="sm" variant="ghost" className="text-warning" onClick={onInstallApk}>
            Update app
          </Button>
        </div>
      )}

      {wirelessOff && (
        <div
          data-testid="wireless-not-configured"
          className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-secondary px-4 py-3 text-secondary-foreground"
        >
          <span className="flex min-w-0 items-center gap-2">
            <Wifi className="size-4 shrink-0" />
            <span>Wireless is not set up for this device. It streams over USB only.</span>
          </span>
          <Button size="sm" onClick={onConfigureWireless}>
            <Wifi />
            Configure Wireless Mode
          </Button>
        </div>
      )}

      <>
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
              disabled={wirelessOff}
              onValueChange={(preferred_transport) => onChange({ preferred_transport })}
            />
          </SettingRow>
          <SettingRow label="Wi-Fi address" tip="IP:port used for wireless streaming to this tablet.">
            <TextInput
              aria-label="Wi-Fi address"
              disabled={wirelessOff}
              className="w-52 font-mono"
              placeholder="192.168.1.50:5555"
              value={p.wifi_address ?? ""}
              onChange={(v) => onChange({ wifi_address: v || undefined })}
            />
          </SettingRow>
        </Section>
        <ModeSection
          values={power}
          onChange={(patch) => onChange(patch)}
          override={powerOverride}
          modeTip={POWER_TIP}
          title="Input mode"
          label="Mode"
        />
      </>

      <Tabs
        label="Device settings"
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "mode", label: labelOf(INPUT_MODES, power.input_mode) },
          { value: "tablet", label: "Android app" },
        ]}
      />

      {tab === "mode" && (
        <ModePanel
          values={power}
          mode={power.input_mode}
          onChange={(patch) => onChange(patch)}
          override={powerOverride}
          displayExtra={
                <Section title="Display & video" hint="Click a grey value to override it for this device">
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
        }
        />
      )}
      {tab === "tablet" && (
        <TabletPanel
          values={power}
          presets={g.curve_presets}
          onPresetsChange={onPresetsChange}
          onChange={(patch) => onChange(patch)}
          override={powerOverride}
        />
      )}


      <div className="flex justify-end pt-2">
        <Button size="sm" variant="ghost" className="text-destructive hover:bg-destructive/10 hover:text-destructive" onClick={() => setConfirmForget(true)}>
          <Trash2 />
          Forget Device
        </Button>
      </div>

      <Dialog open={confirmForget} onOpenChange={setConfirmForget}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>Forget this device?</DialogTitle>
            <DialogDescription>
              {p.nickname?.trim() || device?.model || serial} and its saved settings will be removed from the
              configuration. You can add it again with Add device.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter className="gap-2 sm:gap-2">
            <Button size="sm" variant="ghost" onClick={() => setConfirmForget(false)}>
              Cancel
            </Button>
            <Button
              size="sm"
              variant="destructive"
              onClick={() => {
                setConfirmForget(false);
                onDelete();
              }}
            >
              <Trash2 />
              Forget Device
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
