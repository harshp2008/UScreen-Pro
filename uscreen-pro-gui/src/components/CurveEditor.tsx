import { Save, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { NumberInput, Select, TextInput } from "./ui/controls";
import { BlockRow, type Override } from "./ui/layout";
import { clampBezier, curveFn, curvePath, formatBezier, parseBezier, type Bezier } from "../lib/curves";
import { CURVES } from "../lib/options";
import type { CurvePreset } from "../lib/types";

const CUSTOM = "custom";
const MISSING = "missing";
const DEFAULT_BEZIER: Bezier = { x1: 0.25, y1: 0.1, x2: 0.25, y2: 1 };

interface Props {
  label: string;
  tip?: string;
  /** Stored curve: a preset name, `preset:<name>` or `bezier:x1,y1,x2,y2`. */
  value: string;
  onChange: (value: string) => void;
  presets: CurvePreset[];
  /** Persist the list of saved presets straight away (they are shared by every device). */
  onPresetsChange: (presets: CurvePreset[]) => void;
  override?: Override;
}

/** Name of what a stored value is, for the dropdown. */
function selectValue(value: string, presets: CurvePreset[]): string {
  if (value.startsWith("bezier:")) return CUSTOM;
  if (value.startsWith("preset:")) return presets.some((p) => `preset:${p.name}` === value) ? value : MISSING;
  return value;
}

/** Read-only plot of a curve: the diagonal for reference, the curve itself, start and end points. */
function CurvePreview({ fn }: { fn: ((t: number) => number) | null }) {
  const w = 132;
  const h = 96;
  return (
    <svg
      role="img"
      aria-label="Curve preview"
      viewBox={`0 0 ${w} ${h}`}
      width={w}
      height={h}
      className="shrink-0 rounded-md border border-border bg-muted/40"
    >
      <line x1="8" y1={h - 8} x2={w - 8} y2="8" className="stroke-border" strokeDasharray="3 3" />
      {fn && <path d={curvePath(fn, w, h)} fill="none" className="stroke-primary" strokeWidth="2" strokeLinecap="round" />}
      <circle cx="8" cy={h - 8} r="2.5" className="fill-muted-foreground" />
      <circle cx={w - 8} cy="8" r="2.5" className="fill-muted-foreground" />
    </svg>
  );
}

/**
 * Fade curve picker: the five built-in curves, the user's saved presets and "Custom". Custom shows four
 * number inputs for the control points of a cubic Bezier and a live preview; a custom curve can be saved
 * under a name, after which it is in the dropdown for every device. Saved presets can be deleted.
 */
export function CurveEditor({ label, tip, value, onChange, presets, onPresetsChange, override }: Props) {
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const current = selectValue(value, presets);
  const bezier = parseBezier(value) ?? DEFAULT_BEZIER;
  const options = [
    ...CURVES,
    ...presets.map((p) => ({ value: `preset:${p.name}`, label: p.name })),
    ...(current === MISSING ? [{ value: MISSING, label: `${value.slice(7)} (deleted)` }] : []),
    { value: CUSTOM, label: "Custom…" },
  ];
  const trimmed = name.trim();
  const clash = presets.some((p) => p.name.toLowerCase() === trimmed.toLowerCase());

  const setPoint = (patch: Partial<Bezier>) => onChange(formatBezier({ ...bezier, ...patch }));

  const savePreset = () => {
    if (!trimmed) return;
    const b = clampBezier(bezier);
    const next = [...presets.filter((p) => p.name.toLowerCase() !== trimmed.toLowerCase()), { name: trimmed, ...b }];
    onPresetsChange(next);
    onChange(`preset:${trimmed}`);
    setName("");
    setSaving(false);
  };

  return (
    <BlockRow
      label={label}
      tip={tip}
      override={override}
      summary={options.find((o) => o.value === current)?.label}
    >
      <div className="flex flex-wrap items-start gap-4">
        <CurvePreview fn={curveFn(value, presets)} />
        <div className="min-w-0 flex-1 space-y-2">
          <Select
            label={label}
            value={current}
            options={options}
            className="w-full max-w-56"
            onValueChange={(v) => {
              if (v === MISSING) return;
              setSaving(false);
              onChange(v === CUSTOM ? formatBezier(bezier) : v);
            }}
          />

          {current === CUSTOM && (
            <>
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
                {(
                  [
                    ["x1", 0, 1],
                    ["y1", -2, 3],
                    ["x2", 0, 1],
                    ["y2", -2, 3],
                  ] as const
                ).map(([k, min, max]) => (
                  <label key={k} className="flex items-center gap-1.5 text-xs text-muted-foreground">
                    {k}
                    <NumberInput
                      label={`${label} ${k}`}
                      value={bezier[k]}
                      min={min}
                      max={max}
                      step={0.05}
                      className="w-16"
                      onCommit={(n) => setPoint({ [k]: n })}
                    />
                  </label>
                ))}
              </div>
              {saving ? (
                <div className="flex items-center gap-2">
                  <TextInput
                    aria-label="Preset name"
                    placeholder="My Smooth Fade"
                    className="w-44"
                    value={name}
                    maxLength={40}
                    onChange={setName}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") savePreset();
                    }}
                  />
                  <Button size="sm" onClick={savePreset} disabled={!trimmed}>
                    <Save />
                    {clash ? "Replace" : "Save"}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setSaving(false)}>
                    Cancel
                  </Button>
                </div>
              ) : (
                <Button size="sm" variant="secondary" onClick={() => setSaving(true)}>
                  <Save />
                  Save as Preset
                </Button>
              )}
            </>
          )}

          {presets.length > 0 && (
            <div className="space-y-1 pt-1">
              <div className="text-[11px] font-medium text-muted-foreground">Saved presets</div>
              {presets.map((p) => (
                <div key={p.name} className="flex items-center justify-between gap-2 text-xs">
                  <span className="truncate">{p.name}</span>
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {p.x1}, {p.y1}, {p.x2}, {p.y2}
                  </span>
                  <Button
                    size="icon"
                    variant="ghost"
                    className="size-6 text-muted-foreground hover:text-destructive"
                    aria-label={`Delete preset ${p.name}`}
                    onClick={() => onPresetsChange(presets.filter((q) => q.name !== p.name))}
                  >
                    <Trash2 className="size-3.5" />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </BlockRow>
  );
}
