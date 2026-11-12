/*
 * Per-view scroll memory: remembers how far each view was scrolled, for the length of the session, so switching views does not lose the place.
 * Inputs: a view key and a pixel offset. Storage: sessionStorage, always wrapped so a blocked store only means "not remembered".
 */

/** sessionStorage key prefix under which each view remembers its scroll position. */
const SCROLL_PREFIX = "uscreen-scroll:";

/**
 * The remembered scroll offset in pixels of the view [key] for this session, or 0 when none is stored or storage is unavailable.
 */
export function readScroll(key: string): number {
  try {
    return Number(sessionStorage.getItem(SCROLL_PREFIX + key)) || 0;
  } catch {
    return 0;
  }
}

/**
 * Remembers the scroll offset [top] of the view [key] for the session; silently skipped when storage is unavailable.
 */
export function writeScroll(key: string, top: number) {
  try {
    sessionStorage.setItem(SCROLL_PREFIX + key, String(Math.round(top)));
  } catch {
    /* storage unavailable: scroll just won't be remembered */
  }
}
