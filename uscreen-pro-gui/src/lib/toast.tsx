/**
 * Notifications via the library's own Snackbar (Inverse-Surface container, Inverse-On-Surface text).
 * `toast.*` keeps the call shape the app already used, so call sites only change their import.
 * The host is lifted above the 40px status bar so a message never covers the version number.
 */
import { SnackbarHost, useSnackbarState } from "@bug-on/m3-expressive/feedback";
import type { SnackbarVisuals, UseSnackbarStateReturn } from "@bug-on/m3-expressive/feedback";
import { useEffect } from "react";

let host: UseSnackbarStateReturn["showSnackbar"] | null = null;

function show(message: string, visuals: Partial<SnackbarVisuals> = {}) {
  void host?.({ message, ...visuals });
}

export const toast = {
  success: (message: string, _opts?: { id?: string }) => show(message),
  info: (message: string) => show(message),
  error: (message: string, _opts?: { id?: string }) => show(message, { duration: "long", withDismissAction: true }),
  /** Shown until the next message replaces it in the queue. */
  loading: (message: string, _opts?: { id?: string }) => show(message, { duration: "short" }),
};

export function Notifications() {
  const state = useSnackbarState();
  useEffect(() => {
    host = state.showSnackbar;
    return () => {
      host = null;
    };
  }, [state.showSnackbar]);
  return <SnackbarHost state={state} className="bottom-16" />;
}
