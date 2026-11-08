import { invoke } from "@tauri-apps/api/core";
import { FlaskConical, Pencil, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "../lib/toast";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Select, TextInput } from "./ui/controls";
import { Section, SettingRow, type Override } from "./ui/layout";
import { CUSTOM_ACTION, STYLUS_ACTIONS, isValidStylusAction } from "../lib/options";
import type { StylusCatalog, StylusProfile } from "../lib/types";

/* Radix Select cannot hold an empty value, so "None" travels as this and is stored as no mapping. */
const NONE = "__none__";
const ACTION_MENU = [
  ...STYLUS_ACTIONS.map((o) => (o.value === "" ? { ...o, value: NONE } : o)),
  { value: CUSTOM_ACTION, label: "Custom..." },
];

/** Text of a Tauri error (a string from the backend, or an Error). */
const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

/** Lower-case slug of a name, unique among the taken ids and never one of the built-in ids. */
function idFor(name: string, taken: Set<string>): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40) || "profile";
  let id = base;
  for (let n = 2; taken.has(id) || id === "standard" || id === "samsung_spen"; n++) id = `${base}-${n}`;
  return id;
}

interface Props {
  /** The effective active profile id (the global one, or the device's override). */
  value: string;
  onChange: (id: string) => void;
  /** Present on a device page: the "override the global value" switch of the Active profile row. */
  override?: Override;
}

/**
 * Stylus Profile Manager: choose the active profile and create, edit, test and delete custom ones.
 * Profiles are JSON files the daemon reads from `~/.config/uscreen/stylus_profiles/`; the same list and
 * the same editor exist in the tablet app, and both write the same files.
 */
export function StylusProfileManager({ value, onChange, override }: Props) {
  const [catalog, setCatalog] = useState<StylusCatalog>({ profiles: [], triggers: [] });
  const [editing, setEditing] = useState<StylusProfile | "new" | null>(null);

  const refresh = useCallback(async () => {
    try {
      setCatalog(await invoke<StylusCatalog>("list_stylus_profiles"));
    } catch (e) {
      toast.error(`Could not load stylus profiles: ${errText(e)}`);
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const options = catalog.profiles.map((p) => ({ value: p.id, label: p.name }));
  /* The active id may name a profile that was deleted; keep it visible rather than blank. */
  if (value && !options.some((o) => o.value === value)) options.push({ value, label: `${value} (missing)` });
  const custom = catalog.profiles.filter((p) => !p.builtin);

  async function remove(p: StylusProfile) {
    try {
      await invoke("delete_stylus_profile", { id: p.id });
      if (value === p.id) onChange("standard");
      await refresh();
      toast.success(`Deleted ${p.name}`);
    } catch (e) {
      toast.error(errText(e));
    }
  }

  return (
    <>
      <Section title="Stylus Profile Manager" hint="Map pen buttons and gestures to actions">
        <SettingRow
          label="Active profile"
          tip="What the pen's buttons do. Standard passes them to applications unchanged; the Samsung profile adds S Pen Remote air gestures; a custom profile maps them to clicks and key combinations."
          override={override}
        >
          <Select label="Active stylus profile" value={value} options={options} onValueChange={onChange} />
        </SettingRow>
        {custom.map((p) => (
          <div key={p.id} className="flex min-h-10 items-center justify-between gap-4 px-3" data-testid="custom-profile">
            <div className="min-w-0">
              <div className="truncate text-foreground">{p.name}</div>
              <div className="text-[11px] text-muted-foreground">
                {Object.keys(p.mappings).length} mapped event(s) &middot; {p.id}.json
              </div>
            </div>
            <div className="flex shrink-0 gap-1">
              <Button size="sm" variant="ghost" onClick={() => setEditing(p)}>
                <Pencil />
                Edit
              </Button>
              <Button
                size="sm"
                variant="ghost"
                className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                onClick={() => void remove(p)}
              >
                <Trash2 />
                Delete
              </Button>
            </div>
          </div>
        ))}
        <div className="flex min-h-10 items-center justify-between gap-4 px-3">
          <span className="text-[11px] text-muted-foreground">
            Custom profiles are saved as JSON in ~/.config/uscreen/stylus_profiles/
          </span>
          <Button size="sm" variant="secondary" onClick={() => setEditing("new")}>
            <Plus />
            New profile
          </Button>
        </div>
      </Section>

      {editing && (
        <ProfileEditor
          catalog={catalog}
          existing={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={async (id, wasNew) => {
            setEditing(null);
            await refresh();
            if (wasNew) onChange(id);
          }}
        />
      )}
    </>
  );
}

function ProfileEditor({
  catalog,
  existing,
  onClose,
  onSaved,
}: {
  catalog: StylusCatalog;
  existing: StylusProfile | null;
  onClose: () => void;
  onSaved: (id: string, wasNew: boolean) => Promise<void>;
}) {
  const [name, setName] = useState(existing?.name ?? "");
  const [actions, setActions] = useState<Record<string, string>>({ ...(existing?.mappings ?? {}) });
  /* Triggers whose action is typed by hand rather than picked from the presets. */
  const [customKeys, setCustomKeys] = useState<Set<string>>(
    () =>
      new Set(
        Object.entries(existing?.mappings ?? {})
          .filter(([, v]) => !STYLUS_ACTIONS.some((o) => o.value === v))
          .map(([k]) => k),
      ),
  );
  const [saving, setSaving] = useState(false);

  const invalid = Object.values(actions).some((v) => !isValidStylusAction(v));
  const set = (key: string, v: string) =>
    setActions((a) => {
      const next = { ...a };
      if (v.trim() === "") delete next[key];
      else next[key] = v;
      return next;
    });

  async function test(action: string) {
    try {
      const done = await invoke<string>("test_stylus_action", { action });
      toast.success(`Ran ${done.replace(/^ran /, "")} on this computer`);
    } catch (e) {
      toast.error(errText(e));
    }
  }

  async function save() {
    setSaving(true);
    try {
      const id = existing?.id ?? idFor(name, new Set(catalog.profiles.map((p) => p.id)));
      await invoke("save_stylus_profile", { profile: { id, name: name.trim(), mappings: actions } });
      toast.success(`Saved ${name.trim()}`);
      await onSaved(id, existing === null);
    } catch (e) {
      toast.error(errText(e));
      setSaving(false);
    }
  }

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-h-[90vh] w-[560px] max-w-[96vw] gap-3 overflow-y-auto text-sm">
        <DialogHeader>
          <DialogTitle>{existing ? "Edit stylus profile" : "New stylus profile"}</DialogTitle>
          <DialogDescription>
            Pick what each pen event does. Test runs the action on this computer right now (click into a text field
            first to see a key combination work).
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-1">
          <label htmlFor="profile-name" className="text-xs text-muted-foreground">
            Profile name
          </label>
          <TextInput id="profile-name" className="w-full" value={name} onChange={setName} placeholder="My pen" />
        </div>
        <div className="divide-y divide-border rounded-lg border border-border">
          {catalog.triggers.map((t) => {
            const v = actions[t.key] ?? "";
            const isCustom = customKeys.has(t.key);
            return (
              <div key={t.key} className="space-y-1.5 px-3 py-2" data-testid={`mapping-${t.key}`}>
                <div className="flex items-center justify-between gap-3">
                  <span className="min-w-0 truncate text-foreground">{t.label}</span>
                  <div className="flex shrink-0 items-center gap-2">
                    <Select
                      label={`Action for ${t.label}`}
                      className="w-48"
                      value={isCustom ? CUSTOM_ACTION : v || NONE}
                      options={ACTION_MENU}
                      onValueChange={(choice) => {
                        setCustomKeys((s) => {
                          const next = new Set(s);
                          if (choice === CUSTOM_ACTION) next.add(t.key);
                          else next.delete(t.key);
                          return next;
                        });
                        set(t.key, choice === CUSTOM_ACTION || choice === NONE ? "" : choice);
                      }}
                    />
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={!v || !isValidStylusAction(v)}
                      onClick={() => void test(v)}
                    >
                      <FlaskConical />
                      Test
                    </Button>
                  </div>
                </div>
                {isCustom && (
                  <TextInput
                    aria-label={`Custom action for ${t.label}`}
                    className="w-full font-mono"
                    placeholder="ctrl+alt+t"
                    value={v}
                    onChange={(text) => set(t.key, text)}
                  />
                )}
                {v && !isValidStylusAction(v) && (
                  <p className="text-[11px] text-destructive">
                    Use modifiers (ctrl, shift, alt, meta) joined with +, then one key, e.g. ctrl+shift+z.
                  </p>
                )}
              </div>
            );
          })}
        </div>
        <DialogFooter className="gap-2 sm:gap-2">
          <Button size="sm" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button size="sm" onClick={() => void save()} disabled={!name.trim() || invalid || saving}>
            Save profile
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
