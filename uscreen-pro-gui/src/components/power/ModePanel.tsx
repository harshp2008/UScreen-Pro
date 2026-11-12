/*
 * The input-mode picker (ModeSection) and the settings of the active input mode (ModePanel): stylus, touchpad or screen.
 * Owns the rows and their wording; the values and saving live in App.tsx.
 */
import type { ReactNode } from "react";
import { Segmented, Select } from "../ui/controls";
import { Columns, Section, SettingRow, SliderRow, SwitchRow } from "../ui/layout";
import {
  INPUT_MODES,
  INPUT_MODE_HELP,
  PEN_MENU_HOLD,
  PEN_MENU_STYLE,
  RIGHT_CLICK_METHODS,
  TOUCHPAD_SHAPES,
  STYLUS_STYLES,
  PEN_BUTTONS,
  HAPTIC_STRENGTHS,
  TOUCHPAD_STYLES,
  labelOf,
  modeLabel,
} from "../../lib/options";
import type { InputMode } from "../../lib/types";
import { onOff, overrideOf, type Common } from "./shared";

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
        tip={modeTip ?? "The mode new devices start in. One mode is active per device."}
        hint={INPUT_MODE_HELP[v.input_mode]}
        override={ov("input_mode", modeLabel(v.input_mode, v.stylus_style))}
      >
        <Segmented
          label="Input mode"
          value={v.input_mode}
          options={INPUT_MODES}
          onValueChange={(input_mode) => onChange({ input_mode: input_mode as InputMode })}
        />
      </SettingRow>
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
            tip="How far the pointer moves per finger movement. 100% is the finger's real distance."
            formatValue={(n) => `${Math.round(n)}%`}
            value={v.touchpad_sensitivity}
            min={50}
            max={200}
            step={5}
            onChange={(touchpad_sensitivity) => onChange({ touchpad_sensitivity })}
            override={ov("touchpad_sensitivity", `${v.touchpad_sensitivity}%`)}
          />
          <SettingRow
            label="Touchpad style"
            tip="Clickpad: tap anywhere to click. Classic: the pad only moves the pointer and real buttons sit along the bottom."
            override={ov("touchpad_style", labelOf(TOUCHPAD_STYLES, v.touchpad_style))}
          >
            <Segmented
              label="Touchpad style"
              value={v.touchpad_style}
              options={TOUCHPAD_STYLES}
              onValueChange={(touchpad_style) => onChange({ touchpad_style })}
            />
          </SettingRow>
          {v.touchpad_style === "classic" && (
            <>
              <SwitchRow
                label="Middle button"
                tip="Add a middle button between the left and right ones."
                checked={v.touchpad_middle_button}
                onChange={(touchpad_middle_button) => onChange({ touchpad_middle_button })}
                override={ov("touchpad_middle_button", onOff(v.touchpad_middle_button))}
              />
              <SwitchRow
                label="Tap to click"
                tip="Let a tap on the pad click, as on a modern laptop. Off, like old pads: use the buttons."
                checked={v.touchpad_classic_tap}
                onChange={(touchpad_classic_tap) => onChange({ touchpad_classic_tap })}
                override={ov("touchpad_classic_tap", onOff(v.touchpad_classic_tap))}
              />
            </>
          )}
          <SwitchRow
            label="Touchpad Haptics"
            tip="Vibrate the tablet on clicks. Needs a vibration motor; the tablet app says if it has none."
            checked={v.touchpad_haptics}
            onChange={(touchpad_haptics) => onChange({ touchpad_haptics })}
            override={ov("touchpad_haptics", onOff(v.touchpad_haptics))}
          />
          {v.touchpad_haptics && (
            <SettingRow
              label="Haptic strength"
              tip="How firm the vibration is."
              override={ov("touchpad_haptic_strength", labelOf(HAPTIC_STRENGTHS, v.touchpad_haptic_strength))}
            >
              <Segmented
                label="Haptic strength"
                value={v.touchpad_haptic_strength}
                options={HAPTIC_STRENGTHS}
                onValueChange={(touchpad_haptic_strength) => onChange({ touchpad_haptic_strength })}
              />
            </SettingRow>
          )}
          <SwitchRow
            label="Click sound"
            tip="A short tick from the tablet's speaker on clicks."
            checked={v.touchpad_click_sound}
            onChange={(touchpad_click_sound) => onChange({ touchpad_click_sound })}
            override={ov("touchpad_click_sound", onOff(v.touchpad_click_sound))}
          />
          <SwitchRow
            label="Use Native Gestures (Wayland/libinput)"
            tip="Pass fingers to this computer as a real touchpad (GNOME/KDE gestures). Off: the tablet sends key presses instead."
            checked={v.native_gestures}
            onChange={(native_gestures) => onChange({ native_gestures })}
            override={ov("native_gestures", onOff(v.native_gestures))}
          />
          <SettingRow
            label="Touchpad Shape"
            tip="Which part of the tablet is the touchpad. Touches outside it are ignored."
            override={ov("touchpad_shape", labelOf(TOUCHPAD_SHAPES, v.touchpad_shape))}
          >
            <Segmented
              label="Touchpad shape"
              value={v.touchpad_shape}
              options={TOUCHPAD_SHAPES}
              onValueChange={(touchpad_shape) => onChange({ touchpad_shape })}
            />
          </SettingRow>
          {v.touchpad_style === "clickpad" && (
          <SettingRow
            label="Right Click Method"
            tip="Tap the bottom-right zone, or tap with two fingers. Three fingers is a middle click."
            override={ov("right_click_method", labelOf(RIGHT_CLICK_METHODS, v.right_click_method))}
          >
            <Segmented
              label="Right click method"
              value={v.right_click_method}
              options={RIGHT_CLICK_METHODS}
              onValueChange={(right_click_method) => onChange({ right_click_method })}
            />
          </SettingRow>
          )}
        </Section>
    );
  }
  return (
    <Columns>
      <div>
        <Section title="Pen">
          <SettingRow
            label="Pen acts as"
            tip="Dynamic: a pointer, plus pressure and tilt in drawing apps. Mouse: a plain pointer. Graphics tablet: drawing only."
            override={ov("stylus_style", labelOf(STYLUS_STYLES, v.stylus_style))}
          >
            <Select
              label="Pen acts as"
              value={v.stylus_style}
              options={STYLUS_STYLES}
              className="w-52"
              onValueChange={(stylus_style) => onChange({ stylus_style })}
            />
          </SettingRow>
          {(v.stylus_style === "mouse" || v.stylus_style === "auto") && (
            <>
              <SettingRow
                label="Pen button"
                tip="What the pen's side button does: click, send a shortcut, or open the pen menu."
                override={ov("stylus_button_primary", labelOf(PEN_BUTTONS, v.stylus_button_primary))}
              >
                <Select
                  label="Pen button"
                  value={v.stylus_button_primary}
                  options={PEN_BUTTONS}
                  className="w-40"
                  onValueChange={(stylus_button_primary) => onChange({ stylus_button_primary })}
                />
              </SettingRow>
              <SettingRow
                label="Second pen button"
                tip="For pens with a second side button."
                override={ov("stylus_button_secondary", labelOf(PEN_BUTTONS, v.stylus_button_secondary))}
              >
                <Select
                  label="Second pen button"
                  value={v.stylus_button_secondary}
                  options={PEN_BUTTONS}
                  className="w-40"
                  onValueChange={(stylus_button_secondary) => onChange({ stylus_button_secondary })}
                />
              </SettingRow>
            </>
          )}
        </Section>
        <Section title="Pen menu">
          <SwitchRow
            label="Pen menu"
            tip="Hold the pen still for a small menu: cut, copy, paste, select all, undo. GNOME only."
            checked={v.pen_menu}
            onChange={(pen_menu) => onChange({ pen_menu })}
            override={ov("pen_menu", onOff(v.pen_menu))}
          />
          {v.pen_menu && (
            <>
              <SettingRow
                label="Hold time"
                tip="How long to hold the pen still before the menu appears."
                override={ov("pen_menu_hold_ms", labelOf(PEN_MENU_HOLD, v.pen_menu_hold_ms))}
              >
                <Select
                  label="Hold time for the menu"
                  value={String(v.pen_menu_hold_ms)}
                  options={PEN_MENU_HOLD}
                  className="w-36"
                  onValueChange={(x) => onChange({ pen_menu_hold_ms: Number(x) })}
                />
              </SettingRow>
              <SettingRow
                label="Menu buttons"
                tip="Show the menu buttons as text labels or as icons."
                override={ov("pen_menu_style", labelOf(PEN_MENU_STYLE, v.pen_menu_style))}
              >
                <Select
                  label="Menu buttons"
                  value={v.pen_menu_style}
                  options={PEN_MENU_STYLE}
                  className="w-36"
                  onValueChange={(pen_menu_style) => onChange({ pen_menu_style })}
                />
              </SettingRow>
            </>
          )}
        </Section>
        <Section title="Screen">
          <SwitchRow
            label="Black screen in stylus mode"
            tip="Blank the tablet's screen a few seconds after stylus mode starts. Spares OLED panels."
            checked={v.blank_pen_only}
            onChange={(blank_pen_only) => onChange({ blank_pen_only })}
            override={ov("blank_pen_only", onOff(v.blank_pen_only))}
          />
        </Section>
      </div>
      {stylusExtra}
    </Columns>
  );
}
