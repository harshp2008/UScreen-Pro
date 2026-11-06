/**
 * Thin adapters over @bug-on/m3-expressive. No control here draws its own chrome: every button,
 * field, select, slider, switch and segmented group is the library component, so shape morphing,
 * spring motion and state layers come from the library. The adapters only fix the density
 * (xs / dense) and keep call sites terse.
 */
import { Button as M3Button, ButtonGroup } from "@bug-on/m3-expressive/buttons";
import { Select as M3Select, Slider as M3Slider, Switch as M3Switch, TextField } from "@bug-on/m3-expressive/forms";
import type { ReactNode } from "react";
import type { Option } from "../../lib/options";

/** Join class names, skipping falsy entries. */
export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

/* ---------------------------------------------------------------- Switch */

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  label,
}: {
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return <M3Switch ariaLabel={label} checked={checked} disabled={disabled} onCheckedChange={onCheckedChange} />;
}

/* ---------------------------------------------------------------- Select */

export function Select({
  value,
  onValueChange,
  options,
  disabled,
  label,
  className,
}: {
  value: string;
  onValueChange: (v: string) => void;
  options: Option[];
  disabled?: boolean;
  label: string;
  className?: string;
}) {
  return (
    <M3Select
      aria-label={label}
      dense
      variant="outlined"
      value={value}
      options={options}
      disabled={disabled}
      onChange={onValueChange}
      inputClassName="text-xs"
      menuClassName="text-xs"
      showDividers={false}
      className={className ?? "w-64"}
    />
  );
}

/* ---------------------------------------------------------------- Slider */

/** Native slider: xs track, 4dp handle, Inverse-Surface value bubble while dragging. */
export function Slider({
  value,
  min,
  max,
  step = 1,
  onValueChange,
  label,
  formatValue,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onValueChange: (v: number) => void;
  label: string;
  formatValue?: (v: number) => string;
}) {
  return (
    <M3Slider
      aria-label={label}
      size="xs"
      variant="primary"
      value={value}
      min={min}
      max={max}
      step={step}
      showValueIndicator
      formatValue={formatValue}
      onValueChange={onValueChange}
      className="w-full"
    />
  );
}

/* ------------------------------------------------------------- Segmented */

export function Segmented({
  value,
  onValueChange,
  options,
  label,
}: {
  value: string;
  onValueChange: (v: string) => void;
  options: Option[];
  label: string;
}) {
  return (
    <ButtonGroup variant="connected" size="xs" aria-label={label} showCheck>
      {options.map((o) => (
        <M3Button
          key={o.value}
          value={o.value}
          variant="toggle"
          colorStyle="filled"
          selected={o.value === value}
          onClick={() => onValueChange(o.value)}
        >
          {o.label}
        </M3Button>
      ))}
    </ButtonGroup>
  );
}

/* ----------------------------------------------------------------- Input */

export function TextInput({
  className,
  value,
  onChange,
  type = "text",
  placeholder,
  disabled,
  onKeyDown,
  "aria-label": ariaLabel,
  inputMode,
  min,
  max,
  step,
  maxLength,
  id,
}: {
  className?: string;
  value: string | number;
  onChange?: (value: string) => void;
  type?: "text" | "number";
  placeholder?: string;
  disabled?: boolean;
  onKeyDown?: React.KeyboardEventHandler<HTMLInputElement | HTMLTextAreaElement>;
  "aria-label"?: string;
  inputMode?: "text" | "numeric" | "decimal";
  min?: number | string;
  max?: number | string;
  step?: number | string;
  maxLength?: number;
  id?: string;
}) {
  const mono = className?.includes("font-mono");
  return (
    <TextField
      dense
      variant="outlined"
      type={type}
      noSpinner
      aria-label={ariaLabel ?? placeholder ?? "Value"}
      placeholder={placeholder}
      disabled={disabled}
      value={String(value)}
      onChange={(v) => onChange?.(v)}
      onKeyDown={onKeyDown}
      inputMode={inputMode}
      maxLength={maxLength}
      id={id}
      min={min === undefined ? undefined : String(min)}
      max={max === undefined ? undefined : String(max)}
      step={step === undefined ? undefined : String(step)}
      className={className ?? "w-52"}
      inputClassName={cx("text-xs", mono && "font-mono")}
    />
  );
}

/* ---------------------------------------------------------------- Button */

type Variant = "filled" | "tonal" | "text" | "danger" | "error";

const COLOR_STYLE = { filled: "filled", tonal: "tonal", text: "text", danger: "text", error: "filled" } as const;

/*
 * The library's Button ships a press ripple but no hover layer (BUTTON_COLOR_TOKENS has no hover
 * classes). These are the same hover/active tokens the library applies to its own filled and
 * tonal toggle buttons, passed through the documented `className` prop.
 */
const STATE_LAYER: Record<Variant, string> = {
  filled: "hover:brightness-95 active:brightness-90",
  tonal: "hover:brightness-95 active:brightness-90",
  error: "bg-m3-error text-m3-on-error hover:brightness-90 active:brightness-85",
  text: "hover:bg-m3-primary/8 active:bg-m3-primary/12",
  danger: "text-m3-error hover:bg-m3-error/8 active:bg-m3-error/12",
};

/**
 * Library Button. Default shape is the expressive pill that springs to a rounded square on press.
 * Icons go through the `icon` slot (never as children) so the library can animate and swap them.
 */
export function Button({
  variant = "tonal",
  className,
  icon,
  size = "xs",
  ...props
}: Omit<React.ComponentProps<typeof M3Button>, "colorStyle" | "variant" | "selected" | "icon"> & {
  variant?: Variant;
  icon?: ReactNode;
}) {
  return (
    <M3Button
      type="button"
      colorStyle={COLOR_STYLE[variant]}
      size={size}
      icon={icon}
      className={cx(STATE_LAYER[variant], className)}
      {...props}
    />
  );
}
