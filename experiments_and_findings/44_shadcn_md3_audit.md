# 44. Migration of `uscreen-pro-gui` to shadcn/ui + MD3 tokens: audit

Scope: replace `@bug-on/m3-expressive` + `motion` with shadcn/ui (Radix primitives, `new-york` style, Tailwind v3) themed
by Material Design 3 colour tokens from `material-shadcn`. Method: read of the source tree at `6a6835f`, the library
audit in `43_md3_audit.md`, and the registry output of `https://material-shadcn.vercel.app/r/10B981`.

## BEFORE: violations

| # | Area | Finding | MD3 expectation | shadcn/ui + tokens fix |
|---|------|---------|-----------------|------------------------|
| B1 | Slider | `M3Slider` (`controls.tsx`) renders one tick per `step` (21 dots on Quality, 56 on Bitrate); the library has no `marks` prop; handle is a 4dp pill that depends on the library's spring runtime. | Discrete slider: handle + value label, few or no ticks | shadcn `Slider` = Radix `Slider` (native, keyboard, `step={1}`, `aria-valuenow`). Value is shown in the row text. No ticks. |
| B2 | Dropdown | `M3Select` is a TextField + Menu that needs a `LazyMotion` provider (`main.tsx`); without it the menu stays at `opacity:0 scale:.8`. Rows grow to 57px because the trigger root is 48dp. | Exposed dropdown menu: trigger + menu surface, dense on desktop | shadcn `Select` (Radix Select): portal, collision-aware popper, typeahead. `size="sm"` trigger = 32px. Standard left label + right control, no floating label. |
| B3 | Buttons | Library `Button` has press ripple but no hover state layer (`43` V1). Hover was patched with `hover:brightness-*` in `STATE_LAYER`. | 8% hover / 10% press state layer | shadcn `Button` variants own `hover:bg-primary/90`-style layers from tokens. `default` = Start, `destructive` = Stop. |
| B4 | Density | Rows 48-57px, text fields 48dp (library floor), nav items forced to 36dp with `!h-9`. | Desktop density -2/-3 | `size="sm"` on Button/Select/Input (32px), `text-xs`/`text-sm` labels, `min-h-10` rows. |
| B5 | Sidebar spacing | "Start Daemon" button and "Settings" nav item are siblings with only the drawer's `gap-2`; no margin separating the CTA from navigation. | Distinct groups separated by 16dp | `mb-4` after the daemon button. |
| B6 | Library overrides | `!h-9` x4, `!h-auto`, `!bg-transparent`, `!rounded-none` on `NavigationDrawer*`; `STATE_LAYER` hover hacks; `rounded-[2px]` etc. | No patching of component internals | Own plain markup nav built from Tailwind utilities + token colours; no `!important`. |
| B7 | Dependencies | `@bug-on/m3-expressive` (unofficial, v1.3.x), `motion`, `@tailwindcss/vite` v4 and hand-authored `--md-sys-color-*` map in `theme.css` (127 lines). | Official, maintained primitives | Radix + shadcn (code owned in `src/components/ui`), Tailwind v3, token set generated in `index.css`. |
| B8 | Toasts | Library `Snackbar` host with `bottom-16`; tied to the removed library. | Snackbar, Inverse-Surface | shadcn `sonner` toaster themed through tokens (`--popover` etc.). |
| B9 | Chips | `Chip variant="input"` from the library (App lock list). | Input chip with remove affordance | Token-styled `Badge`-like chip with an `X` icon button. |
| B10 | Tooltip | `PlainTooltip` from the library inside a Radix tooltip. | Plain tooltip | shadcn `Tooltip` (Radix). |

Already correct in the repo and must survive the migration (Part 4): a single Start/Stop Daemon button (no status
card), a dedicated Advanced tab, and an App Locking editor backed by a real `string[]` (no comma-separated string).

## Theme finding (affects the plan)

`npx shadcn@latest add https://material-shadcn.vercel.app/r/10B981` returns background/card/border/secondary in the
emerald-teal hue range (about 165-210), but **`primary` is `oklch(0.492 0.107 46.6)` in light and `oklch(0.836 0.098
47.2)` in dark**: a brown/amber (hue 47), not emerald. `059669` and `10b981` give the same hue, so the generator is
not following the seed for the primary role. With that value "Start Daemon" (`variant="default"`) would be brown, not
green. Resolution: apply the registry theme as instructed, then redefine only the primary-family tokens (`--primary`,
`--primary-foreground`, `--ring`, `--sidebar-primary*`, `--chart-*`) in `src/index.css` with the emerald anchors the
app already used (`#047857` on white light, `#34D399` on `#00382A` dark). Token edits only, no component CSS.

## AFTER

Method: `./build-all.sh` (exit 0), old GUI killed, `~/.local/bin/uscreen-pro-gui` replaced (`cmp` matches `dist/`),
new binary launched and captured with `gnome-screenshot -w` (shot 01). Interaction checks (mouse + keyboard) and
`getBoundingClientRect` / `getComputedStyle` measurements ran against the Vite dev server at 1024x720 with the DEV IPC
mock, which serves the same `src/`; the deployed binary was not driven with synthetic input.

| # | Result | Evidence |
|---|--------|----------|
| B1 | **Fixed.** Radix slider: click sets the value, ArrowLeft/Right step by 1 (18 -> 23 "Efficient"), `aria-valuenow` present, no tick dots. shadcn `Slider` has no `marks` prop (same limit as the old library); none added. | shot 04 |
| B2 | **Fixed.** Encoder dropdown opens on click as a popper surface with 5 options and the current one checked; selecting changes the value; `Escape` closes. No provider needed. Trigger 32px, text 12px, left-aligned row label, no floating label. | shot 03, measured |
| B3 | **Fixed.** `variant="default"` Start Daemon (emerald), `variant="destructive"` Stop Daemon; hover layers come from the variant classes. | `Sidebar.tsx`, shot 01/02 |
| B4 | **Fixed.** Buttons/Select/Input/ToggleGroup 32px, rows 40px (was 48-57px, slider rows ~99px), body 12px. | measured: rows `[40]`, select `32x224` |
| B5 | **Fixed.** 20px between Start Daemon (y 48-80) and Settings (y 100); nav items and buttons share `x=12 w=215`. | measured |
| B6 | **Fixed.** `grep` finds no `!` modifiers, no `bug-on`, `motion`, `md-sys`; `styles.css` and `theme.css` deleted. | `grep` |
| B7 | **Fixed.** `@bug-on/m3-expressive`, `motion`, `@tailwindcss/vite`, `@radix-ui/react-checkbox` removed; Tailwind 3.4, shadcn `new-york` components (button, select, switch, slider, dialog, dropdown-menu, input, separator, tooltip, sonner, toggle-group, badge). | `package.json` |
| B8 | **Fixed.** `toast.*` kept; backed by sonner. Not triggered in the verification run. | build + typecheck |
| B9 | **Fixed.** Removable chips (Badge + X button) over a real `string[]`. | shot 05 |
| B10 | **Fixed.** shadcn Tooltip, inverse (`bg-foreground text-background`). Not hovered in the verification run. | typecheck |

Part 4 items: daemon button (single, no card), Advanced tab with chip list, spacing: all present and verified above.

### Deviations and caveats (read these)

1. **Theme override.** The `10B981` registry theme was applied, then `primary`, `secondary`, `accent`, `ring`,
   `sidebar-*` and `chart-*` were redefined in `src/index.css` (see "Theme finding"). Everything else (background,
   card, popover, muted, border, destructive) is the generated output. `success`/`warning` and `--radius` were added.
2. **Tailwind v3 colour mixing.** The generated tokens are oklch/hex colours, not `H S L` triplets, so
   `tailwind.config.ts` maps them with `color-mix(in oklab, var(--x) calc(<alpha-value> * 100%), transparent)` to keep
   `/90` opacity utilities working. Requires a WebKitGTK with `color-mix` (the deployed app renders it correctly).
3. **`size="sm"` only exists on `Button`.** shadcn `Select`, `Input`, `Switch` and `Slider` have no `size` prop; the
   generated components were edited in place (`h-8`, `text-xs`), which is the intended way to own shadcn code.
4. **`marks` prop does not exist** on shadcn `Slider`; discrete `step={1}` is used, no visual marks.
5. **Not verified:** toast and tooltip visuals, the Add device wizard, the Device settings page and the dark theme in
   the dev server. Dark theme was seen only in the deployed-app shot 01. A single click on a freshly opened tab can
   race the render (the Advanced switch needed a second click in one run); a re-click worked, no app defect was found.
6. Changes are uncommitted on `feat/tauri-md3-frontend`.

Screenshots: `screenshots/44/` (01 deployed Tauri dark, 02 Settings light, 03 dropdown open, 04 slider in use,
05 Advanced with chips).
