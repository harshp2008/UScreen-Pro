/**
 * App-level adapters over the shadcn/ui primitives in this folder. They only reshape props
 * (option arrays, string values, single-value sliders) so call sites stay terse; every control
 * is the shadcn component, sized for a dense desktop layout.
 */
import { useEffect, useState, type KeyboardEventHandler } from "react";
import type { Option } from "@/lib/options";
import { Input } from "@/components/ui/input";
import { Select as UiSelect, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Slider as UiSlider } from "@/components/ui/slider";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { cn } from "@/lib/utils";

/** Exposed dropdown: a trigger showing the current option and a menu surface listing the rest. */
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
    <UiSelect value={value} onValueChange={onValueChange} disabled={disabled}>
      <SelectTrigger aria-label={label} className={cn("w-56", className)}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((o) => (
          <SelectItem key={o.value} value={o.value}>
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </UiSelect>
  );
}

/**
 * shadcn Slider with a value tooltip: an absolutely positioned bubble that follows the thumb's
 * percentage offset and shows while the pointer is over the slider, a drag is active, or the
 * thumb has keyboard focus. The thumb is 16px wide and Radix keeps its centre inside the track, so
 * the offset is corrected by (0.5 - pct) * 16px.
 */
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
  const [hover, setHover] = useState(false);
  const [focus, setFocus] = useState(false);
  const [drag, setDrag] = useState(false);

  useEffect(() => {
    if (!drag) return;
    const end = () => setDrag(false);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, [drag]);

  const pct = (Math.min(max, Math.max(min, value)) - min) / (max - min);
  const show = hover || focus || drag;

  return (
    <div
      className="relative w-full"
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
      onPointerDown={() => setDrag(true)}
      onFocus={() => setFocus(true)}
      onBlur={() => setFocus(false)}
    >
      <div
        role="presentation"
        aria-hidden
        className={cn(
          "pointer-events-none absolute -top-7 z-20 -translate-x-1/2 whitespace-nowrap rounded-md bg-foreground px-2 py-1 text-xs font-medium text-background shadow transition-opacity",
          show ? "opacity-100" : "opacity-0",
        )}
        style={{ left: `calc(${pct * 100}% + ${(0.5 - pct) * 16}px)` }}
      >
        {formatValue ? formatValue(value) : value}
        <span className="absolute -bottom-1 left-1/2 size-2 -translate-x-1/2 rotate-45 bg-foreground" />
      </div>
      <UiSlider
        aria-label={label}
        value={[value]}
        min={min}
        max={max}
        step={step}
        marks
        onValueChange={([v]) => onValueChange(v)}
      />
    </div>
  );
}

/** Single-choice segmented control (ToggleGroup). Re-clicking the active segment keeps it selected. */
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
    <ToggleGroup
      type="single"
      variant="outline"
      size="sm"
      aria-label={label}
      value={value}
      onValueChange={(v) => v && onValueChange(v)}
      className="gap-0"
    >
      {options.map((o) => (
        <ToggleGroupItem
          key={o.value}
          value={o.value}
          className="h-8 rounded-none px-3 text-xs first:rounded-l-md last:rounded-r-md data-[state=on]:bg-primary data-[state=on]:text-primary-foreground"
        >
          {o.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}

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
  onKeyDown?: KeyboardEventHandler<HTMLInputElement>;
  "aria-label"?: string;
  inputMode?: "text" | "numeric" | "decimal";
  min?: number | string;
  max?: number | string;
  step?: number | string;
  maxLength?: number;
  id?: string;
}) {
  return (
    <Input
      type={type}
      aria-label={ariaLabel ?? placeholder ?? "Value"}
      placeholder={placeholder}
      disabled={disabled}
      value={String(value)}
      onChange={(e) => onChange?.(e.target.value)}
      onKeyDown={onKeyDown}
      inputMode={inputMode}
      maxLength={maxLength}
      id={id}
      min={min}
      max={max}
      step={step}
      className={cn("w-52 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none", className)}
    />
  );
}
