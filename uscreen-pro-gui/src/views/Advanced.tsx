/*
 * The Advanced page: the experimental switches (off by default) and the app-lock list. Presentational; state arrives as props from App.tsx.
 */
import { Check } from "lucide-react";
import { useRef, useState } from "react";
import { AppLockList } from "../components/ui/AppLockList";
import { Button } from "@/components/ui/button";
import { PageHeader, Section, SwitchRow } from "../components/ui/layout";
import type { AppConfig } from "../lib/types";

/**
 * Props of the Advanced page: global settings, whether they have unsaved edits, and change and save callbacks.
 */
interface Props {
  config: AppConfig;
  dirty: boolean;
  onChange: (patch: Partial<AppConfig>) => void;
  onSave: () => void;
}

/** Settings that change how windows are managed on the host; off by default. */
export function Advanced({ config: c, dirty, onChange, onSave }: Props) {
  // The array is the stored state (empty = off). The switch reveals the editor; turning it off
  // stashes the list so turning it back on restores it.
  const [enabled, setEnabled] = useState(c.locked_apps.length > 0);
  const stash = useRef<string[]>(c.locked_apps);

  /**
   * Turns the app lock on or off; turning it off remembers the list so turning it on again restores it.
   */
  function toggle(on: boolean) {
    setEnabled(on);
    if (on) onChange({ locked_apps: stash.current });
    else {
      stash.current = c.locked_apps;
      onChange({ locked_apps: [] });
    }
  }

  return (
    <div className="w-full space-y-2">
      <PageHeader title="Advanced">
        <Button size="sm" disabled={!dirty} onClick={onSave}><Check />
          Save
        </Button>
      </PageHeader>

      <Section title="Experimental" hint="Off by default">
        <SwitchRow
          label="Enable App Locking (Mutter)"
          tip="Keep chosen applications on the virtual display; other windows are moved off it."
          checked={enabled}
          onChange={toggle}
        />
        {enabled && (
          <div className="px-3 py-3">
            <p className="mb-2 text-xs text-muted-foreground">
              Application IDs pinned to the virtual display, for example org.gnome.Nautilus or gimp.
            </p>
            <AppLockList apps={c.locked_apps} onChange={(locked_apps) => onChange({ locked_apps })} />
          </div>
        )}
      </Section>
    </div>
  );
}
