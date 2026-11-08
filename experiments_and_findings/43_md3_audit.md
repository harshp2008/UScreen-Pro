# 43. MD3 Expressive audit of `uscreen-pro-gui`

Library: `@bug-on/m3-expressive` 1.3.24. Method: Vite dev server + built-in browser at 1024x720 (the app's default
window), real pointer hover, computed styles read back from the DOM. IPC is stubbed by `src/lib/devMock.ts`
(DEV only). Final verification is repeated in the deployed Tauri binary (see "After").

## BEFORE: violations found

| # | Area | Finding (measured) | Spec / expectation |
|---|------|--------------------|--------------------|
| V1 | Hover state layer, Button | Hovering "Start daemon" (`:hover` true): `background-color` stays `rgb(4,120,87)`, `filter: none`, no overlay element. Same for the tonal "Add device". Library source: `BUTTON_COLOR_TOKENS` for filled/tonal/text carry **no** `hover:` classes; only a press ripple exists. | State layer: on-color at 8% on hover, 10-12% on press/focus |
| V2 | Daemon status | A bordered `rounded-2xl bg-card` card with a status dot ("Host daemon / Stopped") plus a button inside it. Custom composite, not an MD3 pattern; the button is `filled` when stopped and `tonal` when running (no Error role). | One primary CTA; Primary when Start, Error when Stop |
| V3 | Slider handle | `.m3-round-thumb` in `styles.css` forces the handle to a 20px circle with `!important`, adds a `box-shadow` ring and a scale spring. The library handle is a 4dp-wide pill. | Native handle: 4dp pill, width animates on press |
| V4 | Slider ticks | `showTicks` with `step=1`: **21 dots** on Quality (12-32) and **56 dots** on Bitrate. The library renders one tick per step and has no `marks` prop. | Few/no ticks; discrete sliders show a value label |
| V5 | Slider value label | `showValueIndicator={false}`; the value is only text beside the row label. The native bubble (inverse-surface) is never used. | Value indicator uses Inverse-Surface / Inverse-On-Surface |
| V6 | Select | Uses library `Select` (TextField + Menu, correct chrome) but forces `!text-[13px]`, root `rounded-[4px] bg-field`, fixed `w-60/w-64/w-28`; trigger root is **48px**, so every settings row is **57px** tall. | Dense text (12px), density -2 or -3 |
| V7 | Density | Body 12px but rows 57px (select-driven), slider rows ~99px, section gap 8px + 8px title. Reads as a mobile settings list on a 1024x720 window. | Desktop density -2/-3 (heights reduced in 4dp steps) |
| V8 | Library overrides | Sidebar uses `!h-9` x4, `!h-auto`, `!bg-transparent`, `!rounded-none` on `NavigationDrawer*` (library item is `h-14`). Sonner toasts are restyled with ~12 `!important` rules in `styles.css`. | No `!important` patches on library internals |
| V9 | Toast | Sonner (not the library Snackbar), offset hand-tuned; colours faked with CSS rather than Inverse-Surface. | Library Snackbar, positioned above the footer |
| V10 | Sidebar grid | Card is inset by `p-3` inside a `px-4` column while nav items and "Add device" are inset differently (footer wrapper `px-3`), so left/right edges differ by 12px between the Start button and the footer button. | Buttons/inputs share one vertical grid |
| V11 | Hover, other | Nav items, ToggleButtons (segmented), IconButtons, Switch use library tokens with hover layers; Select trigger/TextField hover darkens the outline (library behaviour). Only Button (V1) lacks it. | - |

Root cause of V1: **not** a missing provider (LazyMotion/MotionProvider are present and menus animate). The library's
plain `Button` has press ripple but no hover layer in v1.3.24.

## AFTER: re-audit (same method, then deployed binary)

| # | Result | Evidence |
|---|--------|----------|
| V1 | **Fixed, with a caveat.** Hover on "Start Daemon": `filter: brightness(0.95)` (was `none`), pressed `0.90`. Applied through the documented `className` prop on the native `Button` using the same `hover:brightness-95 active:brightness-90` tokens the library uses for its own filled toggle button; text buttons get `hover:bg-primary/8`. Every `Button` goes through the one adapter, so all of them have it. | `computer` hover + `getComputedStyle` |
| V2 | **Fixed.** Card and dot deleted. One full-width 32dp button at the top of the sidebar: stopped = "Start Daemon" + Play, Primary `#047857`; running = "Stop Daemon" + Square, Error `#BA1A1A` on `#FFFFFF`. PID/state is the tooltip. | shot 02, 05 |
| V3 | **Fixed.** `.m3-round-thumb` CSS deleted. Handle measures 4x44 (library pill). | `getBoundingClientRect` |
| V4 | **Fixed, marks not possible.** `showTicks` removed; no dots. The library has no `marks` prop (ticks are one per `step`), so "ticks at 12/18/24/32" cannot be done without a custom component. | shot 04 |
| V5 | **Fixed.** `showValueIndicator` on, `formatValue` ("CRF 21", "34 Mbps"); bubble is Inverse-Surface. Row text still shows the band name. | shot 04 |
| V6 | **Fixed.** `!text-[13px]`, `rounded-[4px] bg-field` removed; trigger and menu are `text-xs` (12px), menu without dividers (40dp items, was ~41 + rules). | shot 03 |
| V7 | **Fixed to the library floor.** Rows 57 -> 48-49px, slider rows ~99 -> 48px (label left, slider right), Button 32dp, nav items 36dp. Text fields are `dense` = 48dp (density -2 from 56); the library has no smaller size. | measured |
| V8 | **Fixed.** All `!` overrides removed (`!h-9`, `!h-auto`, `!bg-transparent`, `!rounded-none`); `tailwind-merge` in the library resolves plain classes. Sonner CSS (12 `!important` rules) deleted. | `grep` finds none |
| V9 | **Fixed.** Sonner removed (dependency uninstalled); library `SnackbarHost` with `bottom-16`, Inverse-Surface. `toast.*` call sites unchanged. | shot 05 |
| V10 | **Fixed.** Start Daemon, nav items and Add device all `x=12, width=215`. | measured |

Not verifiable here: pointer hover/open/drag inside the native Tauri window (no ydotool daemon, Wayland). Those states were
checked on the identical bundle in the browser pane; shot 01 is the deployed binary at rest (dark).
Residual: the real daemon was not started from the deployed app, so the Stop (red) state is shown from the browser build.
