import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { PlainTooltip } from "@bug-on/m3-expressive/feedback";
import { Info } from "lucide-react";
import type { ReactNode } from "react";
import { QUALITY_MAX, QUALITY_MIN, qualityLabel } from "../../lib/options";
import { Slider, Switch, cx } from "./controls";

export const TooltipProvider = TooltipPrimitive.Provider;

export function InfoTip({ text }: { text: string }) {
  return (
    <TooltipPrimitive.Root delayDuration={150}>
      <TooltipPrimitive.Trigger asChild>
        <button
          type="button"
          aria-label={text}
          className="inline-flex size-3.5 items-center justify-center rounded-full text-on-surface-variant opacity-40 transition-opacity hover:opacity-100 focus-visible:opacity-100"
        >
          <Info className="size-3" />
        </button>
      </TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side="top"
          align="start"
          sideOffset={4}
          className="z-[70]"
        >
          <PlainTooltip maxWidth={256}>{text}</PlainTooltip>
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

/** A titled group of rows. The card is the only container; rows are separated by hairlines. */
export function Section({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) {
  return (
    <section className="space-y-2">
      <div className="flex items-baseline justify-between px-1 pt-2">
        <h3 className="text-xs font-semibold tracking-wide text-primary">{title}</h3>
        {hint && <span className="text-[11px] text-on-surface-variant">{hint}</span>}
      </div>
      <div className="divide-y divide-outline-variant/60 rounded-lg border border-outline-variant bg-card">
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

function OverrideToggle({ override, label }: { override: Override; label: string }) {
  return (
    <div className="flex items-center gap-2" title="Override the global value for this device">
      <Switch label={`Override ${label}`} checked={override.enabled} onCheckedChange={override.onChange} />
    </div>
  );
}

function Label({ label, tip }: { label: string; tip?: string }) {
  return (
    <div className="flex min-w-0 items-center gap-1">
      <span className="truncate text-on-surface">{label}</span>
      {tip && <InfoTip text={tip} />}
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
    <div className="flex min-h-12 items-center justify-between gap-4 px-3">
      <Label label={label} tip={tip} />
      <div className="flex shrink-0 items-center gap-3">
        {showControl ? (
          children
        ) : (
          <span className="text-[11px] text-on-surface-variant">{override.inherited}</span>
        )}
        {override && <OverrideToggle override={override} label={label} />}
      </div>
    </div>
  );
}

/** Label (+ tip) and live value on the left, the native slider on the right: one 48dp row. */
export function SliderRow({
  label,
  tip,
  valueText,
  value,
  min,
  max,
  step,
  onChange,
  formatValue,
  override,
}: {
  label: string;
  tip?: string;
  valueText: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  formatValue?: (v: number) => string;
  override?: Override;
}) {
  const active = !override || override.enabled;
  return (
    <div className="flex min-h-12 items-center justify-between gap-4 px-3">
      <div className="flex min-w-0 flex-col">
        <Label label={label} tip={tip} />
        <span className={cx("truncate text-xs font-medium", active ? "text-primary" : "text-on-surface-variant")}>
          {active ? valueText : override.inherited}
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-3">
        {active && (
          <div className="w-72">
            <Slider label={label} value={value} min={min} max={max} step={step} formatValue={formatValue} onValueChange={onChange} />
          </div>
        )}
        {override && <OverrideToggle override={override} label={label} />}
      </div>
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
      <Switch label={label} checked={checked} onCheckedChange={onChange} />
    </SettingRow>
  );
}

export function PageHeader({ title, subtitle, children }: { title: string; subtitle?: string; children?: ReactNode }) {
  return (
    <div className="flex min-h-9 flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-baseline gap-2">
        <h2 className="truncate text-2xl font-normal leading-8 text-on-surface">{title}</h2>
        {subtitle && <span className="truncate font-mono text-[11px] text-on-surface-variant">{subtitle}</span>}
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
      valueText={`${crf} · ${qualityLabel(crf)}`}
      value={crf}
      min={QUALITY_MIN}
      max={QUALITY_MAX}
      step={1}
      formatValue={(v) => `CRF ${v}`}
      onChange={onChange}
      override={override}
    />
  );
}
