import { Info, RotateCcw } from "lucide-react";
import type { ReactNode } from "react";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipProvider as UiTooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { QUALITY_MAX, QUALITY_MIN, qualityLabel } from "@/lib/options";
import { Slider } from "./controls";

export const TooltipProvider = UiTooltipProvider;

export function InfoTip({ text }: { text: string }) {
  return (
    <Tooltip delayDuration={150}>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={text}
          className="inline-flex size-3.5 items-center justify-center rounded-full text-muted-foreground opacity-50 transition-opacity hover:opacity-100 focus-visible:opacity-100"
        >
          <Info className="size-3" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="top" align="start" className="max-w-64 text-xs">
        {text}
      </TooltipContent>
    </Tooltip>
  );
}

/** A titled group of rows. The card is the only container; rows are separated by hairlines. */
export function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="space-y-2">
      <div className="flex items-baseline justify-between px-1 pt-2">
        <h3 className="text-xs font-semibold tracking-wide text-primary">{title}</h3>
        {hint && <span className="text-[11px] text-muted-foreground">{hint}</span>}
      </div>
      <div className="divide-y divide-border rounded-lg border border-border bg-card">
        {children}
      </div>
    </section>
  );
}

export interface Override {
  enabled: boolean;
  onChange: (enabled: boolean) => void;
  /** Shown while the row inherits the global value. */
  inherited: string;
}

/**
 * What a row that inherits the global value shows where its control would be: the global value in grey.
 * Clicking it turns it into an override for this device (seeded with the global value). It replaces the
 * old switch on the far right, so the controls of every row stay on one right-aligned edge.
 */
export function InheritedValue({ override, label }: { override: Override; label: string }) {
  return (
    <button
      type="button"
      aria-label={`Override ${label} for this device`}
      title="Click to override the global value for this device"
      onClick={() => override.onChange(true)}
      className="rounded-md px-2 py-1 text-[12px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
    >
      {override.inherited}
    </button>
  );
}

/**
 * Row label. An overridden row is highlighted and carries a small reset icon right after the label (never
 * beside the control, which would push it off the right edge); the icon puts the row back on the global value.
 */
export function Label({ label, tip, override }: { label: string; tip?: string; override?: Override }) {
  const overridden = override?.enabled === true;
  return (
    <div className="flex min-w-0 items-center gap-1">
      <span className={cn("truncate", overridden ? "font-medium text-primary" : "text-foreground")}>{label}</span>
      {tip && <InfoTip text={tip} />}
      {overridden && (
        <button
          type="button"
          aria-label={`Reset ${label} to global`}
          title="Reset to global"
          onClick={() => override.onChange(false)}
          className="inline-flex size-5 items-center justify-center rounded-md text-primary transition-colors hover:bg-secondary focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <RotateCcw className="size-3" />
        </button>
      )}
    </div>
  );
}

/** Label (+ tooltip) on the left, control on the right. */
export function SettingRow({
  label,
  tip,
  override,
  children,
}: {
  label: string;
  tip?: string;
  override?: Override;
  children: ReactNode;
}) {
  const showControl = !override || override.enabled;
  return (
    <div className="flex min-h-10 items-center justify-between gap-4 px-3">
      <Label label={label} tip={tip} override={override} />
      <div className="flex shrink-0 items-center">
        {showControl ? children : <InheritedValue override={override} label={label} />}
      </div>
    </div>
  );
}

/**
 * Free-form row: label (+ tooltip) and override switch on top, the control body underneath. For controls
 * too tall for a single line (the curve editor). While a device inherits the global value the body is hidden.
 */
export function BlockRow({
  label,
  tip,
  override,
  summary,
  children,
}: {
  label: string;
  tip?: string;
  override?: Override;
  /** Shown on the right while the body is hidden or as a compact value when it is open. */
  summary?: string;
  children: ReactNode;
}) {
  const active = !override || override.enabled;
  return (
    <div className="space-y-2 px-3 py-2.5">
      <div className="flex min-h-6 items-center justify-between gap-4">
        <Label label={label} tip={tip} override={override} />
        <div className="flex shrink-0 items-center">
          {!active && override && <InheritedValue override={override} label={label} />}
          {active && summary && <span className="text-xs font-medium text-foreground">{summary}</span>}
        </div>
      </div>
      {active && children}
    </div>
  );
}

/**
 * Desktop slider row. Top: label (left) and live value (right). Middle: full-width slider.
 * Bottom: min label (left) and max label (right).
 */
export function SliderRow({
  label,
  tip,
  formatValue,
  value,
  min,
  max,
  step,
  onChange,
  override,
  minLabel,
  maxLabel,
}: {
  label: string;
  tip?: string;
  formatValue: (v: number) => string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  override?: Override;
  minLabel?: string;
  maxLabel?: string;
}) {
  const active = !override || override.enabled;
  return (
    <div className="space-y-1.5 px-3 py-2.5">
      <div className="flex min-h-6 items-center justify-between gap-4">
        <Label label={label} tip={tip} override={override} />
        <div className="flex shrink-0 items-center">
          {active ? (
            <span className="text-xs font-medium tabular-nums text-foreground">{formatValue(value)}</span>
          ) : (
            override && <InheritedValue override={override} label={label} />
          )}
        </div>
      </div>
      {active && (
        <>
          <Slider label={label} value={value} min={min} max={max} step={step} onValueChange={onChange} />
          <div className="flex justify-between text-[11px] text-muted-foreground">
            <span>{minLabel ?? formatValue(min)}</span>
            <span>{maxLabel ?? formatValue(max)}</span>
          </div>
        </>
      )}
    </div>
  );
}

export function SwitchRow({
  label,
  tip,
  checked,
  onChange,
  override,
}: {
  label: string;
  tip?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  override?: Override;
}) {
  return (
    <SettingRow label={label} tip={tip} override={override}>
      <Switch aria-label={label} checked={checked} onCheckedChange={onChange} />
    </SettingRow>
  );
}

export function PageHeader({ title, subtitle, children }: { title: string; subtitle?: string; children?: ReactNode }) {
  return (
    <div className="flex min-h-9 flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-baseline gap-2">
        <h2 className="truncate text-xl font-medium leading-8 text-foreground">{title}</h2>
        {subtitle && <span className="truncate font-mono text-[11px] text-muted-foreground">{subtitle}</span>}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}

/** CRF slider in steps of 1; the band name ("Balanced", ...) updates as the handle moves. */
export function QualityRow({
  quality,
  onChange,
  override,
}: {
  quality: number;
  onChange: (crf: number) => void;
  override?: Override;
}) {
  const crf = Math.min(QUALITY_MAX, Math.max(QUALITY_MIN, Math.round(quality)));
  return (
    <SliderRow
      label="Quality"
      tip="Encoder CRF: lower is sharper and heavier. 12-15 maximum sharpness, 16-20 balanced (default 18), 21-25 efficient, 26+ low bandwidth."
      formatValue={(v) => `${v} — ${qualityLabel(v)}`}
      minLabel="Sharper"
      maxLabel="Smaller"
      value={crf}
      min={QUALITY_MIN}
      max={QUALITY_MAX}
      step={1}
      onChange={onChange}
      override={override}
    />
  );
}
