import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { PlainTooltip } from "@bug-on/m3-expressive/feedback";
import { Info } from "lucide-react";
import type { ReactNode } from "react";
import { QUALITY_STOPS, nearestQualityIndex } from "../../lib/options";
import { Slider, Switch, cx } from "./controls";

export const TooltipProvider = TooltipPrimitive.Provider;

export function InfoTip({ text }: { text: string }) {
  return (
    <TooltipPrimitive.Root delayDuration={150}>
      <TooltipPrimitive.Trigger asChild>
        <button
          type="button"
          aria-label={text}
          className="inline-flex size-4 items-center justify-center rounded-full text-on-surface-variant opacity-40 transition-opacity hover:opacity-100 focus-visible:opacity-100"
        >
          <Info className="size-3.5" />
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
    <div className="flex min-h-9 items-center justify-between gap-4 px-3 py-1">
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

/** Label on the left, value on the right, full-width track underneath. */
export function SliderRow({
  label,
  tip,
  valueText,
  value,
  min,
  max,
  step,
  onChange,
  override,
  ends,
}: {
  label: string;
  tip?: string;
  ends?: [string, string];
  valueText: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  override?: Override;
}) {
  const active = !override || override.enabled;
  return (
    <div className="px-3 py-2">
      <div className="flex min-h-6 items-center justify-between gap-4">
        <Label label={label} tip={tip} />
        <div className="flex shrink-0 items-center gap-3">
          <span className={cx("font-mono text-[11px]", active ? "text-on-surface" : "text-on-surface-variant")}>
            {active ? valueText : override.inherited}
          </span>
          {override && <OverrideToggle override={override} label={label} />}
        </div>
      </div>
      {active && (
        <>
          <Slider label={label} value={value} min={min} max={max} step={step} onValueChange={onChange} />
          {ends && (
            <div className="flex justify-between text-[10px] text-on-surface-variant">
              <span>{ends[0]}</span>
              <span>{ends[1]}</span>
            </div>
          )}
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
      <Switch label={label} checked={checked} onCheckedChange={onChange} />
    </SettingRow>
  );
}

export function PageHeader({ title, subtitle, children }: { title: string; subtitle?: string; children?: ReactNode }) {
  return (
    <div className="flex min-h-9 flex-wrap items-center justify-between gap-3">
      <div className="flex min-w-0 items-baseline gap-2">
        <h2 className="truncate text-base font-semibold text-on-surface">{title}</h2>
        {subtitle && <span className="truncate font-mono text-[11px] text-on-surface-variant">{subtitle}</span>}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}

/** Qualitative quality control: five stops mapped to the encoder CRF/QP values the backend expects. */
export function QualityRow({
  quality,
  onChange,
  override,
}: {
  quality: number;
  onChange: (crf: number) => void;
  override?: Override;
}) {
  const index = nearestQualityIndex(quality);
  return (
    <SliderRow
      label="Quality"
      tip="Trades text sharpness against bandwidth and encoder load. Balanced (CRF 18) is the default."
      valueText={QUALITY_STOPS[index].label}
      value={index}
      min={0}
      max={QUALITY_STOPS.length - 1}
      ends={["Sharper", "Smaller"]}
      onChange={(i) => onChange(Number(QUALITY_STOPS[i].value))}
      override={override}
    />
  );
}
