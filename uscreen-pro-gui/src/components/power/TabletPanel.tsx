/*
 * Tablet-app, power and idle-dimming settings, the same for every input mode.
 * Owns the rows and their wording; the values and saving live in App.tsx.
 */
import { CurveEditor } from "../CurveEditor";
import { NumberInput, Segmented } from "../ui/controls";
import { Columns, Section, SettingRow, SwitchRow } from "../ui/layout";
import {
  CURVES,
  FADE_MAX_MS,
  FADE_MIN_MS,
  IDLE_TIMEOUT_MAX,
  IDLE_TIMEOUT_MIN,
  ORIENTATIONS,
  THEME_MODES,
  labelOf,
} from "../../lib/options";
import type { CurvePreset } from "../../lib/types";
import { onOff, overrideOf, type Common } from "./shared";

/** `3000` ms as "3 s" (and 1500 as "1.5 s"). */
const seconds = (ms: number) => `${ms / 1000} s`;

/** Tablet-app, power and idle-dimming settings; the same for every mode. */
export function TabletPanel({
  values: v,
  onChange,
  override,
  presets,
  onPresetsChange,
}: Common & { presets: CurvePreset[]; onPresetsChange: (presets: CurvePreset[]) => void }) {
  const hint = override ? "Click a grey value to override it for this device" : undefined;
  const ov = overrideOf({ values: v, onChange, override });
  return (
    <>
      {hint && <p className="px-1 pt-2 text-[11px] text-muted-foreground">{hint}</p>}
      <Columns>
      <Section title="Android app">
        <SettingRow
          label="Orientation"
          tip="Landscape direction of the tablet app. Camera down is the usual way to hold it for drawing."
          override={ov("orientation", labelOf(ORIENTATIONS, v.orientation))}
        >
          <Segmented
            label="Orientation"
            value={v.orientation}
            options={ORIENTATIONS}
            onValueChange={(orientation) => onChange({ orientation })}
          />
        </SettingRow>
        <SettingRow
          label="Theme"
          tip="Light, dark or the tablet's own system setting for the tablet app."
          override={ov("theme_mode", labelOf(THEME_MODES, v.theme_mode))}
        >
          <Segmented
            label="Theme"
            value={v.theme_mode}
            options={THEME_MODES}
            onValueChange={(theme_mode) => onChange({ theme_mode })}
          />
        </SettingRow>
        <SwitchRow
          label="Immersive mode"
          tip="Hide the settings button and draw under the camera cutout. Back opens settings."
          checked={v.immersive_mode}
          onChange={(immersive_mode) => onChange({ immersive_mode })}
          override={ov("immersive_mode", onOff(v.immersive_mode))}
        />
      </Section>

      <Section title="Power & battery" hint={hint}>
        <SwitchRow
          label="Battery saver on disconnect"
          tip="After 2 minutes without the computer, stop retrying and dim to 10%. Tap Retry to reconnect."
          checked={v.battery_saver_on_disconnect}
          onChange={(battery_saver_on_disconnect) => onChange({ battery_saver_on_disconnect })}
          override={ov("battery_saver_on_disconnect", onOff(v.battery_saver_on_disconnect))}
        />
        <SwitchRow
          label="Require Device Admin Lock"
          tip="Also lock the tablet in battery saver. Needs the Device Admin permission."
          checked={v.battery_saver_lock}
          onChange={(battery_saver_lock) => onChange({ battery_saver_lock })}
          override={ov("battery_saver_lock", onOff(v.battery_saver_lock))}
        />
        <SwitchRow
          label="Keep connected in background"
          tip="Keep the stream and touch connection open while the tablet app is switched away. Uses more battery."
          checked={v.keep_connected_in_background}
          onChange={(keep_connected_in_background) => onChange({ keep_connected_in_background })}
          override={ov("keep_connected_in_background", onOff(v.keep_connected_in_background))}
        />
        <SwitchRow
          label="Keep display alive when locked"
          tip="Hold a wake lock and keep the tablet app on screen over the tablet's lock screen."
          checked={v.keep_display_alive_when_locked}
          onChange={(keep_display_alive_when_locked) => onChange({ keep_display_alive_when_locked })}
          override={ov("keep_display_alive_when_locked", onOff(v.keep_display_alive_when_locked))}
        />
        <SwitchRow
          label="Touch on lock screen"
          tip="Off: a locked tablet shows a cover and sends no touch. On: touch keeps working while it is locked."
          checked={v.touch_on_lock_screen}
          onChange={(touch_on_lock_screen) => onChange({ touch_on_lock_screen })}
          override={ov("touch_on_lock_screen", onOff(v.touch_on_lock_screen))}
        />
      </Section>

      </Columns>
      {override && v.input_mode !== "display" ? (
        <p className="px-1 pt-3 text-[11px] text-muted-foreground">Idle dimming applies in Extended Display mode only.</p>
      ) : (
      <Columns>
        <Section title="Dim when idle" hint="Extended Display mode only">
        <SwitchRow
          label="Dim when idle"
          tip="Fade the tablet down when the pointer is away and it is unused. Touch or the pointer wakes it."
          checked={v.idle_dim_enabled}
          onChange={(idle_dim_enabled) => onChange({ idle_dim_enabled })}
          override={ov("idle_dim_enabled", onOff(v.idle_dim_enabled))}
        />
        <SettingRow
          label="Idle timeout"
          tip={`Seconds of no use before dimming (${IDLE_TIMEOUT_MIN}-${IDLE_TIMEOUT_MAX}).`}
          override={ov("idle_dim_timeout_secs", `${v.idle_dim_timeout_secs} s`)}
        >
          <NumberInput
            label="Idle timeout in seconds"
            unit="s"
            value={v.idle_dim_timeout_secs}
            min={IDLE_TIMEOUT_MIN}
            max={IDLE_TIMEOUT_MAX}
            onCommit={(idle_dim_timeout_secs) => onChange({ idle_dim_timeout_secs })}
          />
        </SettingRow>
        <CurveEditor
          label="Dim curve"
          tip="Shape of the fade down to 5% brightness."
          value={v.dim_curve}
          onChange={(dim_curve) => onChange({ dim_curve })}
          presets={presets}
          onPresetsChange={onPresetsChange}
          override={ov("dim_curve", curveName(v.dim_curve))}
        />
        <SettingRow
          label="Dim duration"
          tip="How long the fade down takes."
          override={ov("dim_duration_ms", seconds(v.dim_duration_ms))}
        >
          <NumberInput
            label="Dim duration in seconds"
            unit="s"
            step={0.1}
            value={v.dim_duration_ms / 1000}
            min={FADE_MIN_MS / 1000}
            max={FADE_MAX_MS / 1000}
            onCommit={(s) => onChange({ dim_duration_ms: Math.round(s * 1000) })}
          />
        </SettingRow>
        </Section>
        <Section title="Wake up">
        <CurveEditor
          label="Undim curve"
          tip="Shape of the fade back up to full brightness."
          value={v.undim_curve}
          onChange={(undim_curve) => onChange({ undim_curve })}
          presets={presets}
          onPresetsChange={onPresetsChange}
          override={ov("undim_curve", curveName(v.undim_curve))}
        />
        <SettingRow
          label="Undim duration"
          tip="How long the fade back up takes. Short wakes quickly."
          override={ov("undim_duration_ms", seconds(v.undim_duration_ms))}
        >
          <NumberInput
            label="Undim duration in seconds"
            unit="s"
            step={0.1}
            value={v.undim_duration_ms / 1000}
            min={FADE_MIN_MS / 1000}
            max={FADE_MAX_MS / 1000}
            onCommit={(s) => onChange({ undim_duration_ms: Math.round(s * 1000) })}
          />
        </SettingRow>
        </Section>
      </Columns>
      )}
    </>
  );
}

/** Short name of a stored curve for the "Global: ..." hint. */
function curveName(value: string): string {
  if (value.startsWith("preset:")) return value.slice(7);
  if (value.startsWith("bezier:")) return "Custom";
  return labelOf(CURVES, value);
}
