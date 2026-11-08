/*
 * Presentational building blocks of the Add Device wizard: the progress log, status notes, numbered
 * instruction lists and the "enable USB debugging" timeline. They hold no state and call no
 * backend, which is why they live apart from the wizard's step logic.
 */
import { Check, Info, LoaderCircle, MousePointerClick, Settings, ToggleRight, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** The backend's step-by-step progress strings, shown verbatim; the newest one spins while work continues. */
export function StepLog({ lines, running }: { lines: string[]; running: boolean }) {
  if (lines.length === 0) return null;
  return (
    <ul className="space-y-1" data-testid="step-log">
      {lines.map((line, i) => {
        const last = i === lines.length - 1;
        return (
          <li key={`${i}-${line}`} className="flex items-center gap-2 text-foreground">
            {last && running ? (
              <LoaderCircle className="size-3.5 shrink-0 animate-spin text-primary" />
            ) : (
              <Check className="size-3.5 shrink-0 text-success" />
            )}
            <span>{line}</span>
          </li>
        );
      })}
    </ul>
  );
}

export function Note({ tone = "neutral", children }: { tone?: "neutral" | "warning" | "error" | "success"; children: ReactNode }) {
  const tones = {
    neutral: "bg-muted text-foreground",
    warning: "border border-warning/50 bg-warning/15 text-foreground",
    error: "border border-destructive/50 bg-destructive/15 text-foreground",
    success: "border border-success/50 bg-success/15 text-foreground",
  };
  return <div className={cn("rounded-lg px-3 py-2 text-xs leading-relaxed", tones[tone])}>{children}</div>;
}

export function Instructions({ items }: { items: ReactNode[] }) {
  return (
    <ol className="space-y-2 text-muted-foreground">
      {items.map((item, i) => (
        <li key={i} className="flex gap-2">
          <span className="flex size-4 shrink-0 items-center justify-center rounded-[2px] bg-muted font-mono text-[10px] text-foreground">
            {i + 1}
          </span>
          <span>{item}</span>
        </li>
      ))}
    </ol>
  );
}

const DEBUG_STEPS: { icon: LucideIcon; text: ReactNode }[] = [
  { icon: Settings, text: <>Open <b>Settings</b> on your device.</> },
  { icon: Info, text: <>Go to <b>About Phone</b> (or About Tablet).</> },
  { icon: MousePointerClick, text: <>Tap <b>Build Number</b> 7 times until it says &lsquo;You are a developer&rsquo;.</> },
  { icon: ToggleRight, text: <>Go back and open <b>Developer Options</b>, then toggle <b>USB Debugging</b> ON.</> },
];

/** Vertical timeline: a large icon per step joined by a rail. */
export function DebugTimeline() {
  return (
    <ol>
      {DEBUG_STEPS.map(({ icon: Icon, text }, i) => (
        <li key={i} className="flex gap-3">
          <div className="flex flex-col items-center">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-secondary text-secondary-foreground">
              <Icon className="size-5" />
            </span>
            {i < DEBUG_STEPS.length - 1 && <span className="my-1 w-px flex-1 bg-border" />}
          </div>
          <div className={cn("min-w-0 flex-1 pt-2.5 leading-snug text-foreground", i < DEBUG_STEPS.length - 1 && "pb-6")}>
            <span className="mr-1.5 font-mono text-[11px] text-muted-foreground">{i + 1}.</span>
            {text}
          </div>
        </li>
      ))}
    </ol>
  );
}
