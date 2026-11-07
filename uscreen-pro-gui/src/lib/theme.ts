import { useCallback, useEffect, useState } from "react";

export type ThemePref = "light" | "dark" | "system";

const KEY = "uscreen-theme";
const QUERY = "(prefers-color-scheme: dark)";

export function readThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* storage unavailable: fall through to the default */
  }
  return "system";
}

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
    const onChange = () => setDark(apply("system"));
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [pref]);

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
