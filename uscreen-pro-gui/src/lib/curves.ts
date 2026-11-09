import type { CurvePreset } from "./types";

/** Control points of a cubic Bezier fade through (0,0) and (1,1). */
export interface Bezier {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** Limits the host applies: x in 0..1, y in -2..3. */
export const BEZIER_Y_MIN = -2;
export const BEZIER_Y_MAX = 3;

/** Limits [n] to [lo]..[hi]. */
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));
const round3 = (n: number) => Math.round(n * 1000) / 1000;

/** The stored form of a custom curve: `bezier:x1,y1,x2,y2`. */
export function formatBezier(b: Bezier): string {
  const c = clampBezier(b);
  return `bezier:${c.x1},${c.y1},${c.x2},${c.y2}`;
}

/** Keep control points inside what the tablet's PathInterpolator accepts. */
export function clampBezier(b: Bezier): Bezier {
  return {
    x1: round3(clamp(b.x1, 0, 1)),
    y1: round3(clamp(b.y1, BEZIER_Y_MIN, BEZIER_Y_MAX)),
    x2: round3(clamp(b.x2, 0, 1)),
    y2: round3(clamp(b.y2, BEZIER_Y_MIN, BEZIER_Y_MAX)),
  };
}

/** Parse `bezier:x1,y1,x2,y2`; null for anything else. */
export function parseBezier(value: string): Bezier | null {
  if (!value.startsWith("bezier:")) return null;
  const n = value.slice(7).split(",").map((p) => Number(p.trim()));
  if (n.length !== 4 || n.some((x) => !Number.isFinite(x))) return null;
  return clampBezier({ x1: n[0], y1: n[1], x2: n[2], y2: n[3] });
}

/** y(x) of a CSS cubic-bezier(x1, y1, x2, y2), solved by bisection on the x polynomial. */
export function bezierFn({ x1, y1, x2, y2 }: Bezier): (x: number) => number {
  /** One coordinate of a cubic Bezier at parameter [t] with control values [a] and [b]. */
  const cubic = (a: number, b: number, t: number) => 3 * a * (1 - t) ** 2 * t + 3 * b * (1 - t) * t ** 2 + t ** 3;
  return (x) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let lo = 0;
    let hi = 1;
    for (let i = 0; i < 40; i++) {
      const mid = (lo + hi) / 2;
      if (cubic(x1, x2, mid) < x) lo = mid;
      else hi = mid;
    }
    return cubic(y1, y2, (lo + hi) / 2);
  };
}

/** The preset curves, matching the tablet's interpolators (linear, accelerate, decelerate, accelerate-decelerate, exponential). */
const NAMED: Record<string, (t: number) => number> = {
  linear: (t) => t,
  ease_in: (t) => t * t,
  ease_out: (t) => 1 - (1 - t) * (1 - t),
  ease_in_out: (t) => Math.cos((t + 1) * Math.PI) / 2 + 0.5,
  exponential: (t) => (2 ** (10 * t) - 1) / 1023,
};

/** Evaluate any stored curve value (preset name, `preset:<name>` or `bezier:...`) for the preview; null if it cannot be resolved. */
export function curveFn(value: string, presets: CurvePreset[]): ((t: number) => number) | null {
  if (value.startsWith("preset:")) {
    const p = presets.find((q) => q.name === value.slice(7));
    return p ? bezierFn(p) : null;
  }
  const b = parseBezier(value);
  if (b) return bezierFn(b);
  return NAMED[value] ?? null;
}

/** Vertical extent of a curve's samples, at least 0..1, so an overshooting curve still fits the preview. */
export function curveRange(fn: (t: number) => number): { lo: number; hi: number } {
  let lo = 0;
  let hi = 1;
  for (let i = 0; i <= 48; i++) {
    const y = fn(i / 48);
    lo = Math.min(lo, y);
    hi = Math.max(hi, y);
  }
  return { lo, hi };
}

/** SVG path of a curve, sampled at 48 points, fitted into a `w` x `h` box with `pad` around it. */
export function curvePath(fn: (t: number) => number, w: number, h: number, pad = 8): string {
  const { lo, hi } = curveRange(fn);
  const pts: string[] = [];
  for (let i = 0; i <= 48; i++) {
    const t = i / 48;
    const y = (fn(t) - lo) / (hi - lo);
    pts.push(`${i === 0 ? "M" : "L"}${(pad + t * (w - 2 * pad)).toFixed(1)},${(h - pad - y * (h - 2 * pad)).toFixed(1)}`);
  }
  return pts.join(" ");
}
