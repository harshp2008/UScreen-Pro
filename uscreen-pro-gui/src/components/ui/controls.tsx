import * as SelectPrimitive from "@radix-ui/react-select";
import * as SliderPrimitive from "@radix-ui/react-slider";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import { Check, ChevronDown } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes } from "react";
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
  return (
    <SwitchPrimitive.Root
      aria-label={label}
      checked={checked}
      disabled={disabled}
      onCheckedChange={onCheckedChange}
      className={cx(
        "relative h-[18px] w-8 shrink-0 rounded border transition-colors disabled:opacity-40",
        checked
          ? "border-secondary bg-secondary"
          : "border-outline bg-surface-highest",
      )}
    >
      <SwitchPrimitive.Thumb
        className={cx(
          "block h-3 rounded-[2px] transition-all",
          checked
            ? "ml-[16px] w-3 bg-on-secondary"
            : "ml-[2px] w-2.5 bg-outline",
        )}
      />
    </SwitchPrimitive.Root>
  );
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
    <SelectPrimitive.Root value={value} onValueChange={onValueChange} disabled={disabled}>
      <SelectPrimitive.Trigger
        aria-label={label}
        className={cx(
          "inline-flex h-7 items-center justify-between gap-2 rounded border border-outline-variant",
          "bg-surface px-2 text-xs text-on-surface hover:border-outline",
          "data-[state=open]:border-primary disabled:opacity-40",
          className ?? "w-52",
        )}
      >
        <SelectPrimitive.Value />
        <SelectPrimitive.Icon>
          <ChevronDown className="size-3.5 text-on-surface-variant" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={4}
          className="z-[60] max-h-64 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-lg border border-outline-variant bg-surface-container py-1 text-xs text-on-surface"
        >
          <SelectPrimitive.Viewport>
            {options.map((o) => (
              <SelectPrimitive.Item
                key={o.value}
                value={o.value}
                className="flex h-7 cursor-pointer items-center justify-between gap-3 px-2.5 outline-none data-[highlighted]:bg-surface-highest data-[state=checked]:text-secondary"
              >
                <SelectPrimitive.ItemText>{o.label}</SelectPrimitive.ItemText>
                <SelectPrimitive.ItemIndicator>
                  <Check className="size-3.5" />
                </SelectPrimitive.ItemIndicator>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

/* ---------------------------------------------------------------- Slider */

/** Bare track + handle. Label and value live in `SliderRow`, never inside the track. */
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
    <SliderPrimitive.Root
      aria-label={label}
      value={[value]}
      min={min}
      max={max}
      step={step}
      onValueChange={([v]) => onValueChange(v)}
      className="relative flex h-5 w-full touch-none items-center"
    >
      <SliderPrimitive.Track className="relative h-1 grow rounded-[2px] bg-surface-highest">
        <SliderPrimitive.Range className="absolute h-full rounded-[2px] bg-primary" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb className="block h-4 w-1 rounded-[2px] bg-primary hover:opacity-80" />
    </SliderPrimitive.Root>
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
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex h-7 overflow-hidden rounded border border-outline-variant"
    >
      {options.map((o, i) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onValueChange(o.value)}
            className={cx(
              "flex items-center gap-1 px-2.5 text-xs transition-colors",
              i > 0 && "border-l border-outline-variant",
              on
                ? "bg-secondary-container text-on-secondary-container"
                : "text-on-surface-variant hover:bg-surface-high",
            )}
          >
            {on && <Check className="size-3" />}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/* ----------------------------------------------------------------- Input */

export function TextInput({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      type="text"
      spellCheck={false}
      {...props}
      className={cx(
        "h-7 rounded border border-outline-variant bg-surface px-2 text-xs text-on-surface",
        "placeholder:text-outline hover:border-outline focus:border-primary focus:outline-none",
        className ?? "w-52",
      )}
    />
  );
}

/* ---------------------------------------------------------------- Button */

type Variant = "filled" | "tonal" | "text" | "danger";

const VARIANTS: Record<Variant, string> = {
  filled: "bg-primary text-on-primary hover:opacity-90 disabled:bg-surface-highest disabled:text-outline",
  tonal: "bg-primary-container text-on-primary-container hover:opacity-90 disabled:opacity-40",
  text: "text-primary hover:bg-surface-high disabled:text-outline",
  danger: "text-error hover:bg-error-container disabled:opacity-40",
};

export function Button({
  variant = "tonal",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      type="button"
      {...props}
      className={cx(
        "inline-flex h-7 items-center justify-center gap-1.5 rounded px-3 text-xs font-medium transition-[border-radius,background-color,opacity] duration-150 hover:rounded-lg",
        VARIANTS[variant],
        className,
      )}
    />
  );
}
