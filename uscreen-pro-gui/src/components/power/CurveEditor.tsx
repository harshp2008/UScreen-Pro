/**
 * CurveEditor.tsx: edits one fade or response curve as a preset or a custom cubic Bezier.
 * Inputs: the stored curve value and the saved presets. Output: `onChange` with a preset name, `preset:<name>` or `bezier:x1,y1,x2,y2`.
 * Does not own the preset list (lib/options.ts) or the Bezier maths (lib/curves.ts).
 */
import { Save, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { NumberInput, Select, TextInput } from "../ui/controls";
import { BlockRow, type Override } from "../ui/layout";
import { bezierFn, clampBezier, curveFn, curvePath, formatBezier, parseBezier, type Bezier } from "../../lib/curves";
import { CURVES } from "../../lib/options";
import type { CurvePreset } from "../../lib/types";

/** Select value meaning "the curve is a hand-made Bezier, not a named preset". */
const CUSTOM = "custom";
/** Select value for a saved preset name that no longer exists in the list. */
const MISSING = "missing";
/** Starting handles for a new custom curve (CSS "ease": quick start, long gentle finish). */
const DEFAULT_BEZIER: Bezier = { x1: 0.25, y1: 0.1, x2: 0.25, y2: 1 };

/**
 * Props of the curve editor: the stored curve value, the saved presets and callbacks to change either.
 */
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

/** Visible vertical range of the draggable plot; the host accepts more (-2..3), the numbers below reach it. */
const PLOT_Y_MIN = -0.5;
/** Top edge of the plot, in curve units (1 is a finished fade), so a handle above 1 is still drawn. */
const PLOT_Y_MAX = 1.5;

/**
 * The editable curve: drag the two round handles (or focus one and use the arrow keys) to shape the fade. Time runs left to
 * right, the fade's progress bottom to top. It reports the changed control points of the cubic Bezier.
 */
function DragPlot({ bezier, onChange }: { bezier: Bezier; onChange: (patch: Partial<Bezier>) => void }) {
  const W = 220;
  const H = 170;
  const PAD = 16;
  const ref = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<1 | 2 | null>(null);
  const px = (x: number) => PAD + x * (W - 2 * PAD);
  const py = (y: number) => H - PAD - ((y - PLOT_Y_MIN) / (PLOT_Y_MAX - PLOT_Y_MIN)) * (H - 2 * PAD);
  const fn = bezierFn(bezier);
  const path = Array.from({ length: 49 }, (_, i) => {
    const t = i / 48;
    return `${i === 0 ? "M" : "L"}${px(t).toFixed(1)},${py(fn(t)).toFixed(1)}`;
  }).join(" ");

  /** Sets handle [which] to the pointer position. */
  const moveTo = (which: 1 | 2, clientX: number, clientY: number) => {
    const r = ref.current?.getBoundingClientRect();
    if (!r || r.width === 0) return;
    const cx = ((clientX - r.left) / r.width) * W;
    const cy = ((clientY - r.top) / r.height) * H;
    const x = Math.min(1, Math.max(0, (cx - PAD) / (W - 2 * PAD)));
    const y = PLOT_Y_MIN + ((H - PAD - cy) / (H - 2 * PAD)) * (PLOT_Y_MAX - PLOT_Y_MIN);
    onChange(which === 1 ? { x1: x, y1: y } : { x2: x, y2: y });
  };

  /** Nudges handle [which] by the arrow key pressed. */
  const nudge = (which: 1 | 2, e: React.KeyboardEvent) => {
    const step = e.shiftKey ? 0.1 : 0.02;
    const dx = e.key === "ArrowRight" ? step : e.key === "ArrowLeft" ? -step : 0;
    const dy = e.key === "ArrowUp" ? step : e.key === "ArrowDown" ? -step : 0;
    if (!dx && !dy) return;
    e.preventDefault();
    onChange(which === 1 ? { x1: bezier.x1 + dx, y1: bezier.y1 + dy } : { x2: bezier.x2 + dx, y2: bezier.y2 + dy });
  };

  const handles = [
    { which: 1 as const, x: bezier.x1, y: bezier.y1, from: [0, 0] },
    { which: 2 as const, x: bezier.x2, y: bezier.y2, from: [1, 1] },
  ];
  return (
    <svg
      ref={ref}
      role="group"
      aria-label="Drag the two handles to shape the curve"
      viewBox={`0 0 ${W} ${H}`}
      width={W}
      height={H}
      className="max-w-full shrink-0 touch-none select-none rounded-md border border-border bg-muted/40"
      onPointerMove={(e) => drag && moveTo(drag, e.clientX, e.clientY)}
      onPointerUp={() => setDrag(null)}
      onPointerCancel={() => setDrag(null)}
    >
      <line x1={px(0)} y1={py(0)} x2={px(1)} y2={py(0)} className="stroke-border" />
      <line x1={px(0)} y1={py(1)} x2={px(1)} y2={py(1)} className="stroke-border" />
      <line x1={px(0)} y1={py(0)} x2={px(1)} y2={py(1)} className="stroke-border" strokeDasharray="3 3" />
      {handles.map((h) => (
        <line key={`l${h.which}`} x1={px(h.from[0])} y1={py(h.from[1])} x2={px(h.x)} y2={py(h.y)} className="stroke-muted-foreground" strokeWidth="1" />
      ))}
      <path d={path} fill="none" className="stroke-primary" strokeWidth="2.5" strokeLinecap="round" />
      <circle cx={px(0)} cy={py(0)} r="3" className="fill-muted-foreground" />
      <circle cx={px(1)} cy={py(1)} r="3" className="fill-muted-foreground" />
      {handles.map((h) => (
        <circle
          key={h.which}
          tabIndex={0}
          role="slider"
          aria-label={`Handle ${h.which}: time ${h.x.toFixed(2)}, progress ${h.y.toFixed(2)}`}
          aria-valuenow={Math.round(h.y * 100)}
          cx={px(h.x)}
          cy={py(Math.min(PLOT_Y_MAX, Math.max(PLOT_Y_MIN, h.y)))}
          r="7"
          className={`cursor-grab fill-background stroke-primary outline-none focus-visible:stroke-foreground ${drag === h.which ? "cursor-grabbing" : ""}`}
          strokeWidth="2.5"
          onPointerDown={(e) => {
            (e.currentTarget as SVGCircleElement).setPointerCapture(e.pointerId);
            setDrag(h.which);
          }}
          onKeyDown={(e) => nudge(h.which, e)}
        />
      ))}
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

  /** Changes some control points of the custom curve and reports the new `bezier:` string. */
  const setPoint = (patch: Partial<Bezier>) => onChange(formatBezier({ ...bezier, ...patch }));

  /**
   * Saves the current custom curve under the typed name (replacing a preset of the same name, ignoring case) and selects it.
   */
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
        {current === CUSTOM ? <DragPlot bezier={bezier} onChange={setPoint} /> : <CurvePreview fn={curveFn(value, presets)} />}
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
              <details className="text-xs">
                <summary className="cursor-pointer text-muted-foreground hover:text-foreground">Exact values</summary>
                <div className="pt-1.5">
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
                </div>
              </details>
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
