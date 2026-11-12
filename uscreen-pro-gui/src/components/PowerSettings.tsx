/*
 * Settings sections shared by the global Settings page and a device page: input mode, display and power, battery, idle dimming and fade curves.
 * This file only re-exports, so views keep importing from one place; the sections live in power/ (shared.ts, ModePanel.tsx, TabletPanel.tsx).
 */
export { ModePanel, ModeSection } from "./power/ModePanel";
export { POWER_TIP, type PowerValues } from "./power/shared";
export { TabletPanel } from "./power/TabletPanel";
