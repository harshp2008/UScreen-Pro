import { useCallback, useEffect, useState } from "react";

/** User's theme choice; `system` follows the OS. */
export type ThemePref = "light" | "dark" | "system";

/** localStorage key holding the chosen theme ("light", "dark" or "system"). */
const KEY = "uscreen-theme";
/** Media query that reports whether the operating system prefers a dark theme. */
const QUERY = "(prefers-color-scheme: dark)";

/** Stored theme preference, defaulting to `system` when storage is unavailable or holds junk. */
export function readThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* storage unavailable: fall through to the default */
  }
  return "system";
}

/** Whether the system prefers a dark colour scheme. */
const systemDark = () => window.matchMedia(QUERY).matches;

/** Put the `.dark` class (the library's baseline scheme key, see index.css) in step with the preference. */
function apply(pref: ThemePref): boolean {
  const dark = pref === "system" ? systemDark() : pref === "dark";
  document.documentElement.classList.toggle("dark", dark);
  return dark;
}

/** Theme preference with a System (Auto) mode that follows the OS and reacts live when the OS flips. */
export function useTheme() {
  const [pref, setPrefState] = useState<ThemePref>(readThemePref);
  const [dark, setDark] = useState(() => apply(readThemePref()));

  useEffect(() => {
    setDark(apply(pref));
    if (pref !== "system") return;
    const mq = window.matchMedia(QUERY);
    /**
     * Re-applies the system scheme when the OS preference changes (only while the choice is `system`).
     */
    const onChange = () => setDark(apply("system"));
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [pref]);

  /**
   * Stores and applies a new theme choice; if storage is unavailable the choice just does not persist.
   */
  const setPref = useCallback((next: ThemePref) => {
    setPrefState(next);
    try {
      localStorage.setItem(KEY, next);
    } catch {
      /* the choice just won't persist */
    }
  }, []);

  return { pref, dark, setPref };
}
