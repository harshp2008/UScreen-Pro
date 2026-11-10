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

/** Plain shadcn Slider adapted to a single numeric value. */
export function Slider({
  value,
  min,
  max,
  step = 1,
  onValueChange,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onValueChange: (v: number) => void;
  label: string;
}) {
  return (
    <UiSlider
      aria-label={label}
      value={[value]}
      min={min}
      max={max}
      step={step}
      onValueChange={([v]) => onValueChange(v)}
    />
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

/** Single-line text input in the app's style; forwards value, change and key handlers. */
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

/**
 * Number field for exact values (1500 ms, 60 s). The text is kept as typed and only committed, clamped
 * to [min, max], on blur or Enter, so typing "1500" never passes through a clamped "1".
 */
export function NumberInput({
  value,
  onCommit,
  min,
  max,
  step = 1,
  unit,
  label,
  className,
  disabled,
}: {
  value: number;
  onCommit: (v: number) => void;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  label: string;
  className?: string;
  disabled?: boolean;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  /**
   * Applies the typed number: a non-number reverts to the current value, anything else is clamped to the range and reported only when it changed.
   */
  const commit = () => {
    const n = Number(draft);
    if (!Number.isFinite(n) || draft.trim() === "") {
      setDraft(String(value));
      return;
    }
    const next = Math.min(max, Math.max(min, n));
    setDraft(String(next));
    if (next !== value) onCommit(next);
  };
  return (
    <div className="flex items-center gap-1.5">
      <Input
        type="number"
        aria-label={label}
        disabled={disabled}
        value={draft}
        min={min}
        max={max}
        step={step}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
        className={cn("w-24 font-mono [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none", className)}
      />
      {unit && <span className="text-xs text-muted-foreground">{unit}</span>}
    </div>
  );
}
