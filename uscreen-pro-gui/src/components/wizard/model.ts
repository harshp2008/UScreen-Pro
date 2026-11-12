/*
 * Fixed vocabulary of the Add Device wizard: the step list and step indices, the small state types, and two constants shared by the
 * steps. No React and no backend calls; the wizard's state and logic stay in AddDeviceWizard.tsx.
 */

/* Step 0 is the landing screen; steps 1-5 are the USB flow (the no-cable wireless path jumps to Finish). */
export const STEPS = ["Prepare Device", "Connect USB", "Install", "Wireless", "Finish"] as const;
/** Index of the wireless-pairing step in the wizard (steps are zero-based). */
export const WIRELESS_STEP = 4;
/** Index of the final step of the wizard. */
export const FINISH_STEP = 5;
/** Wizard step: 0 landing, 1 Prepare Device, 2 Connect USB, 3 Install, 4 Wireless, 5 Finish. */
export type Step = 0 | 1 | 2 | 3 | 4 | 5;
/** How the device is being added: with a cable first (`usb`) or the no-cable wireless path. */
export type Via = "usb" | "wireless";
/** Progress of a background operation (installing the app, or the wireless setup). */
export type InstallState = "idle" | "running" | "ok" | "failed";
/** The transport the device prefers once added: automatic, USB or Wi-Fi. */
export type Transport = "auto" | "usb" | "wifi";
/** Which wireless method is shown: automatic pairing or the Tailscale address. */
export type WirelessTab = "auto" | "tailscale";
/* Backend error prefix of a failed router reachability check (see pairing.rs). */
export const ROUTER_BLOCKED_PREFIX = "ROUTER_BLOCKED|";
/** Tailscale download page, opened from a button in the wireless step. */
export const TAILSCALE_DOWNLOAD_URL = "https://tailscale.com/download";
