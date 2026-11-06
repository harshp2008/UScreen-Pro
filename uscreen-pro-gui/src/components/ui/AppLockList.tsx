import { Chip } from "@bug-on/m3-expressive/forms";
import { Plus } from "lucide-react";
import { useState } from "react";
import { Button, TextInput } from "./controls";

/** Reverse-DNS style IDs and plain binary names: letters, digits, dot, dash, underscore. */
const APP_ID = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

/**
 * Editable list of application IDs: a field plus "Add", and one removable chip per ID in a
 * vertical list below. The list is a real array end to end (no comma-separated string).
 */
export function AppLockList({ apps, onChange }: { apps: string[]; onChange: (apps: string[]) => void }) {
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

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
        <Button variant="tonal" disabled={!draft.trim()} onClick={add} icon={<Plus className="size-4" />}>
          Add
        </Button>
      </div>
      {error && <p className="px-1 text-xs text-error">{error}</p>}
      {apps.length === 0 ? (
        <p className="px-1 text-xs text-on-surface-variant">No applications locked yet.</p>
      ) : (
        <ul className="flex flex-col items-start gap-2" aria-label="Locked applications">
          {apps.map((id) => (
            <li key={id}>
              <Chip
                variant="input"
                label={<span className="font-mono">{id}</span>}
                onRemove={() => onChange(apps.filter((a) => a !== id))}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
