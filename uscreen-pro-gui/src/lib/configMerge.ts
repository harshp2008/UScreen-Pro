/*
 * Settings-file bookkeeping for the window: the three-way merge of tablet-side edits into the open window, the list of settings that
 * wait for Apply, and the fingerprints used to tell what changed. Pure functions over `AppConfig`; no React, no Tauri.
 */
import type { AppConfig, TabletProfile } from "./types";

/** JSON with sorted keys, so re-adding a key later does not look like an edit. */
export const stable = (v: unknown) =>
  JSON.stringify(v, (_k, x) =>
    x && typeof x === "object" && !Array.isArray(x)
      ? Object.fromEntries(Object.entries(x).sort(([a], [b]) => a.localeCompare(b)))
      : x,
  );

/**
 * Three-way merge of the settings file as it is now (`remote`) into the window's copy (`local`),
 * given what the window last loaded (`base`): a value the user has not touched here takes the new
 * file value (so changes made on the tablet appear), a value they edited here stays theirs.
 */
export function mergeConfig(base: AppConfig, local: AppConfig, remote: AppConfig): AppConfig {
  /**
   * The merged value: the local one if it differs from the base (edited here), otherwise the one now in the file.
   */
  const take = (b: unknown, l: unknown, r: unknown) => (stable(l) !== stable(b) ? l : r);
  const merged: Record<string, unknown> = {};
  const keys = new Set([...Object.keys(remote), ...Object.keys(local)]);
  for (const k of keys) {
    if (k === "tablets") continue;
    /** Reads the top-level setting [k] from an object. */
    const rec = (o: unknown) => (o as Record<string, unknown>)[k];
    merged[k] = take(rec(base), rec(local), rec(remote));
  }
  const tablets: Record<string, TabletProfile> = {};
  for (const serial of new Set([...Object.keys(remote.tablets), ...Object.keys(local.tablets)])) {
    const r = remote.tablets[serial];
    const l = local.tablets[serial];
    const b = base.tablets[serial];
    if (!r) {
      // Gone from the file: keep it only if there are unsaved edits to it here.
      if (l && b && stable(l) !== stable(b)) tablets[serial] = l;
      continue;
    }
    if (!l || !b) {
      tablets[serial] = r;
      continue;
    }
    const profile: Record<string, unknown> = {};
    for (const k of new Set([...Object.keys(r), ...Object.keys(l)])) {
      /** Reads the profile setting [k] from an object. */
      const rec = (o: unknown) => (o as Record<string, unknown>)[k];
      const v = take(rec(b), rec(l), rec(r));
      if (v !== undefined) profile[k] = v;
    }
    tablets[serial] = profile as unknown as TabletProfile;
  }
  return { ...(merged as unknown as AppConfig), tablets };
}

/**
 * Settings whose change restarts the video stream or the daemon. Every other setting saves by itself a moment after it
 * is changed; these wait for an explicit Apply so the stream does not restart while you are still choosing.
 */
export const HEAVY = new Set([
  "encoder", "fps", "bitrate", "quality", "stream_scale", "width", "height", "position", "ten_bit", "vaapi_device",
  "pointer_handoff", "auto_resolution", "video_port", "input_port", "max_tablets", "max_performance_mode",
  "hevc_low_latency_tuning", "software_cursor", "keep_alive", "input_mode", "pen_only", "require_token", "wifi_address",
  "preferred_transport", "wireless_configured", "allow_legacy_fallback", "auto_enable_wifi_adb", "locked_apps",
]);

/** The top-level keys whose values differ between two objects. */
export function changedKeys(a: object, b: object): string[] {
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  return [...new Set([...Object.keys(x), ...Object.keys(y)])].filter((k) => stable(x[k]) !== stable(y[k]));
}

/** Fingerprint of the global settings only; per-device edits have their own Save button. */
export const globalsKey = (c: AppConfig) => JSON.stringify({ ...c, tablets: {} });
