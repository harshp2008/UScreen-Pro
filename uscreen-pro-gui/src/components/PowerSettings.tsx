import type { ReactNode } from "react";
import { CurveEditor } from "./CurveEditor";
import { NumberInput, Segmented } from "./ui/controls";
import { Columns, Section, SettingRow, SliderRow, SwitchRow, type Override } from "./ui/layout";
import {
  CURVES,
  FADE_MAX_MS,
  FADE_MIN_MS,
  IDLE_TIMEOUT_MAX,
  IDLE_TIMEOUT_MIN,
  INPUT_MODES,
  INPUT_MODE_HELP,
  ORIENTATIONS,
  RIGHT_CLICK_METHODS,
  TOUCHPAD_SHAPES,
  STYLUS_STYLES,
  PEN_BUTTONS,
  THEME_MODES,
  labelOf,
} from "../lib/options";
import type { CurvePreset, InputMode } from "../lib/types";

/** The settings that exist both globally and as a per-device override; every one is also on the tablet. */
export interface PowerValues {
  input_mode: InputMode;
  touch_input: boolean;
  blank_pen_only: boolean;
  immersive_mode: boolean;
  show_stats: boolean;
  orientation: string;
  theme_mode: string;
  keep_connected_in_background: boolean;
  keep_display_alive_when_locked: boolean;
  touch_on_lock_screen: boolean;
  battery_saver_on_disconnect: boolean;
  battery_saver_lock: boolean;
  idle_dim_enabled: boolean;
  idle_dim_timeout_secs: number;
  dim_curve: string;
  undim_curve: string;
  dim_duration_ms: number;
  undim_duration_ms: number;
  touchpad_haptics: boolean;
  native_gestures: boolean;
  touchpad_sensitivity: number;
  right_click_method: string;
  touchpad_shape: string;
  stylus_style: string;
  stylus_button_primary: string;
  stylus_button_secondary: string;
}

/** Tooltip of the Input mode row of a device: where the power goes and what to do about it. */
export const POWER_TIP =
  "Streaming at high resolutions/framerates consumes significant power. If your device charges slowly, try lowering the FPS, or use Touchpad Mode.";

const onOff = (v: boolean) => (v ? "On" : "Off");

interface Common {
  /** Effective values: the global ones on the Settings page, override-or-global on a device page. */
  values: PowerValues;
  onChange: (patch: Partial<PowerValues>) => void;
  /** Present on a device page: builds the "override the global value" switch of a row. */
  override?: (key: keyof PowerValues, inherited: string) => Override;
}

const overrideOf = (c: Common) => (key: keyof PowerValues, inherited: string) => c.override?.(key, inherited);

/** The input-mode row with its one-line help. Global: the default mode for new devices. Device: its live mode. */
export function ModeSection({ values: v, onChange, override, modeTip, title, label }: Common & {
  modeTip?: string;
  title: string;
  label: string;
}) {
  const ov = overrideOf({ values: v, onChange, override });
  return (
    <Section title={title} hint={override ? "Click a grey value to override it for this device" : undefined}>
      <SettingRow
        label={label}
        tip={
          modeTip ??
          "The mode new devices start in. Exactly one is active per device: a second screen, a laptop touchpad (no video, saves battery) or a graphics tablet (no video)."
        }
        override={ov("input_mode", labelOf(INPUT_MODES, v.input_mode))}
      >
        <Segmented
          label="Input mode"
          value={v.input_mode}
          options={INPUT_MODES}
          onValueChange={(input_mode) => onChange({ input_mode: input_mode as InputMode })}
        />
      </SettingRow>
      <p className="px-3 pb-2 text-[11px] text-muted-foreground">{INPUT_MODE_HELP[v.input_mode]}</p>
    </Section>
  );
}

/** The settings of one input mode. Only the mode being shown is rendered. */
export function ModePanel({
  values: v,
  onChange,
  override,
  mode,
  displayExtra,
  stylusExtra,
}: Common & {
  mode: InputMode;
  /** Extended Display video and layout sections (global and device pages build their own). */
  displayExtra?: ReactNode;
  /** Drawing-tablet and pen-menu sections (global only). */
  stylusExtra?: ReactNode;
}) {
  const ov = overrideOf({ values: v, onChange, override });
  if (mode === "display") {
    return (
      <Columns>
        {displayExtra}
        <Section title="Tablet screen">
        <SwitchRow
          label="Touch input"
          tip="Off sends only the pen, so a resting hand does nothing."
          checked={v.touch_input}
          onChange={(touch_input) => onChange({ touch_input })}
          override={ov("touch_input", onOff(v.touch_input))}
        />
        <SwitchRow
          label="Show stats overlay"
          tip="FPS and bandwidth in the corner of the tablet's screen, measured on the video stream."
          checked={v.show_stats}
          onChange={(show_stats) => onChange({ show_stats })}
          override={ov("show_stats", onOff(v.show_stats))}
        />
      </Section>
      </Columns>
    );
  }
  if (mode === "touchpad") {
    return (
      <Section title="Touchpad">
          <SliderRow
            label="Sensitivity"
            tip="How far the pointer and swipes travel per finger movement on the tablet (100% is the finger's real distance on the tablet). Applies with native gestures. Pointer acceleration on top of it is your desktop's touchpad speed setting."
            formatValue={(n) => `${Math.round(n)}%`}
            value={v.touchpad_sensitivity}
            min={50}
            max={200}
            step={5}
            onChange={(touchpad_sensitivity) => onChange({ touchpad_sensitivity })}
            override={ov("touchpad_sensitivity", `${v.touchpad_sensitivity}%`)}
          />
          <SwitchRow
            label="Touchpad Haptics"
            tip="A light 50 ms vibration on the tablet for left, right and middle clicks."
            checked={v.touchpad_haptics}
            onChange={(touchpad_haptics) => onChange({ touchpad_haptics })}
            override={ov("touchpad_haptics", onOff(v.touchpad_haptics))}
          />
          <SwitchRow
            label="Use Native Gestures (Wayland/libinput)"
            tip="On: the tablet's fingers are passed to this computer as a real touchpad, so motion, taps, scrolling, pinch zoom and 3 / 4 finger workspace swipes are handled by GNOME or KDE exactly as on a laptop trackpad (the workspace follows your hand). Off, or on X11 and desktops that do nothing with libinput gestures: the tablet recognises gestures itself and sends key presses (Super + Page Up / Down, Ctrl + scroll)."
            checked={v.native_gestures}
            onChange={(native_gestures) => onChange({ native_gestures })}
            override={ov("native_gestures", onOff(v.native_gestures))}
          />
          <SettingRow
            label="Touchpad Shape"
            tip="Which part of the tablet acts as the touchpad. Full Screen is all of it; Standard is the largest centred 4:3 rectangle and Wide the largest centred 16:9 one. The tablet draws the shape and ignores touches outside it."
            override={ov("touchpad_shape", labelOf(TOUCHPAD_SHAPES, v.touchpad_shape))}
          >
            <Segmented
              label="Touchpad shape"
              value={v.touchpad_shape}
              options={TOUCHPAD_SHAPES}
              onValueChange={(touchpad_shape) => onChange({ touchpad_shape })}
            />
          </SettingRow>
          <SettingRow
            label="Right Click Method"
            tip="Bottom Right Zone: tap the right half of the strip along the bottom edge of the touch area (a faint line marks it). Two-Finger Tap: tap with two fingers. Two-finger tap is also a right click and three-finger tap a middle click. Pinch zooms, and swiping three or four fingers sideways switches workspace (up or down: overview)."
            override={ov("right_click_method", labelOf(RIGHT_CLICK_METHODS, v.right_click_method))}
          >
            <Segmented
              label="Right click method"
              value={v.right_click_method}
              options={RIGHT_CLICK_METHODS}
              onValueChange={(right_click_method) => onChange({ right_click_method })}
            />
          </SettingRow>
        </Section>
    );
  }
  return (
    <Columns>
      <Section title="Stylus">
        <>
            <SettingRow
              label="Pen acts as"
              tip="Auto: a visible cursor follows the pen everywhere and text selection works; drawing programs (Krita, GIMP, ...) get pressure and tilt, other apps get a mouse click. Ctrl+Alt+P flips the choice for the focused app. Mouse: the pen is a pointer. The tip is the left button and stays down while you drag, so you can select text and move windows; hovering moves a visible cursor. Graphics tablet: a pressure and tilt pen for drawing programs."
              override={ov("stylus_style", labelOf(STYLUS_STYLES, v.stylus_style))}
            >
              <Segmented
                label="Pen acts as"
                value={v.stylus_style}
                options={STYLUS_STYLES}
                onValueChange={(stylus_style) => onChange({ stylus_style })}
              />
            </SettingRow>
            {(v.stylus_style === "mouse" || v.stylus_style === "auto") && (
              <>
                <SettingRow
                  label="Pen button"
                  tip="What the pen's side button does while the pen acts as a mouse."
                  override={ov("stylus_button_primary", labelOf(PEN_BUTTONS, v.stylus_button_primary))}
                >
                  <Segmented
                    label="Pen button"
                    value={v.stylus_button_primary}
                    options={PEN_BUTTONS}
                    onValueChange={(stylus_button_primary) => onChange({ stylus_button_primary })}
                  />
                </SettingRow>
                <SettingRow
                  label="Second pen button"
                  tip="For pens with a second side button."
                  override={ov("stylus_button_secondary", labelOf(PEN_BUTTONS, v.stylus_button_secondary))}
                >
                  <Segmented
                    label="Second pen button"
                    value={v.stylus_button_secondary}
                    options={PEN_BUTTONS}
                    onValueChange={(stylus_button_secondary) => onChange({ stylus_button_secondary })}
                  />
                </SettingRow>
              </>
            )}
        </>
        <SwitchRow
          label="Black screen in stylus mode"
          tip="Nothing is streamed, so the tablet's screen goes black a few seconds after stylus mode starts (spares OLED panels)."
          checked={v.blank_pen_only}
          onChange={(blank_pen_only) => onChange({ blank_pen_only })}
          override={ov("blank_pen_only", onOff(v.blank_pen_only))}
        />
      </Section>
      {stylusExtra}
    </Columns>
  );
}

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
      <Section title="Tablet app">
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
          tip="Hide the settings button on the tablet and draw under the camera cutout. The Back button opens settings."
          checked={v.immersive_mode}
          onChange={(immersive_mode) => onChange({ immersive_mode })}
          override={ov("immersive_mode", onOff(v.immersive_mode))}
        />
      </Section>

      <Section title="Power & battery" hint={hint}>
        <SwitchRow
          label="Battery saver on disconnect"
          tip="If the tablet cannot reach this computer for 2 minutes it stops retrying and dims to 10%. The user taps Retry to reconnect."
          checked={v.battery_saver_on_disconnect}
          onChange={(battery_saver_on_disconnect) => onChange({ battery_saver_on_disconnect })}
          override={ov("battery_saver_on_disconnect", onOff(v.battery_saver_on_disconnect))}
        />
        <SwitchRow
          label="Require Device Admin Lock"
          tip="Off: battery saver only dims the screen and stops retrying. On: it also locks the tablet, which needs the Device Admin permission (the tablet asks for it)."
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
      <Columns>
        <Section title="Dim when idle" hint="Dims the tablet while the pointer is elsewhere">
        <SwitchRow
          label="Dim when idle"
          tip="Fade the tablet's screen down once the pointer has left it and the tablet has gone unused for the timeout. Touching the tablet or moving the pointer back fades it up again."
          checked={v.idle_dim_enabled}
          onChange={(idle_dim_enabled) => onChange({ idle_dim_enabled })}
          override={ov("idle_dim_enabled", onOff(v.idle_dim_enabled))}
        />
        <SettingRow
          label="Idle timeout"
          tip={`How long the pointer must stay away before the tablet dims (${IDLE_TIMEOUT_MIN}-${IDLE_TIMEOUT_MAX} seconds).`}
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
          tip={`Length of the fade down, in milliseconds (${FADE_MIN_MS}-${FADE_MAX_MS}). Type an exact value such as 1500.`}
          override={ov("dim_duration_ms", `${v.dim_duration_ms} ms`)}
        >
          <NumberInput
            label="Dim duration in milliseconds"
            unit="ms"
            step={100}
            value={v.dim_duration_ms}
            min={FADE_MIN_MS}
            max={FADE_MAX_MS}
            onCommit={(dim_duration_ms) => onChange({ dim_duration_ms })}
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
          tip={`Length of the fade back up, in milliseconds (${FADE_MIN_MS}-${FADE_MAX_MS}). Keep it short for a quick wake.`}
          override={ov("undim_duration_ms", `${v.undim_duration_ms} ms`)}
        >
          <NumberInput
            label="Undim duration in milliseconds"
            unit="ms"
            step={100}
            value={v.undim_duration_ms}
            min={FADE_MIN_MS}
            max={FADE_MAX_MS}
            onCommit={(undim_duration_ms) => onChange({ undim_duration_ms })}
          />
        </SettingRow>
        </Section>
      </Columns>
    </>
  );
}

/** Short name of a stored curve for the "Global: ..." hint. */
function curveName(value: string): string {
  if (value.startsWith("preset:")) return value.slice(7);
  if (value.startsWith("bezier:")) return "Custom";
  return labelOf(CURVES, value);
}
