/**
 * AppLockList.tsx: the editable list of application IDs behind the `locked_apps` setting (Advanced and per-device settings).
 * Input/output: a string array and its change callback. Validation of one ID is the `APP_ID` pattern below.
 */
import { Plus, X } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { TextInput } from "./controls";

/** Reverse-DNS style IDs and plain binary names: letters, digits, dot, dash, underscore. */
const APP_ID = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

/**
 * Editable list of application IDs: a field plus "Add", and one removable chip per ID in a
 * wrapping row below. The list is a real array end to end (no comma-separated string).
 */
export function AppLockList({ apps, onChange }: { apps: string[]; onChange: (apps: string[]) => void }) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  /**
   * Adds the typed application id to the list after checking its characters and that it is not already there; shows the problem otherwise.
   */
  function add() {
    const id = draft.trim();
    if (!id) return;
    if (!APP_ID.test(id)) return setError("Use letters, digits, dots, dashes or underscores, e.g. org.gnome.Nautilus.");
    if (apps.includes(id)) return setError(`${id} is already in the list.`);
    onChange([...apps, id]);
    setDraft("");
    setError(null);
  }

  return (
    <div className="w-full space-y-2">
      <div className="flex items-center gap-2">
        <TextInput
          aria-label="Application ID"
          className="min-w-0 flex-1 font-mono"
          placeholder="org.gnome.Nautilus"
          value={draft}
          onChange={(v) => {
            setDraft(v);
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button variant="secondary" size="sm" disabled={!draft.trim()} onClick={add}>
          <Plus />
          Add
        </Button>
      </div>
      {error && <p className="px-1 text-xs text-destructive">{error}</p>}
      {apps.length === 0 ? (
        <p className="px-1 text-xs text-muted-foreground">No applications locked yet.</p>
      ) : (
        <ul className="flex flex-wrap gap-1.5" aria-label="Locked applications">
          {apps.map((id) => (
            <li key={id}>
              <Badge variant="secondary" className="gap-1 py-0.5 pl-2.5 pr-1 font-mono text-xs font-normal">
                {id}
                <button
                  type="button"
                  aria-label={`Remove ${id}`}
                  onClick={() => onChange(apps.filter((a) => a !== id))}
                  className="inline-flex size-4 items-center justify-center rounded-full hover:bg-foreground/10 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                >
                  <X className="size-3" />
                </button>
              </Badge>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
