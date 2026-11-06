# 42. Broken dropdowns, slider scale, Advanced tab

## Dropdowns did not open
- **Hypothesis:** the `options` prop was mis-passed.
- **Method:** opened the Encoder select in a browser preview and inspected the DOM.
- **Raw log:** caret rotated (state `open`), a `role="menu"` panel existed at the right position with the five options, but computed `opacity: 0; transform: matrix(0.8,...)`, i.e. frozen at its `initial` animation state.
- **Cause:** the library's `m.*` elements need a `LazyMotion` provider. Experiment 41 dropped `MD3ThemeProvider` (to keep exact palette values), which was also the only thing supplying `LazyMotion`. Menus, and any other `m.*` animation, stayed at their start frame. `options` was correct.
- **Fix:** `main.tsx` wraps the app in `LazyMotion` (`domMax`) and the library's `MotionProvider`; no colour engine.
- **Verified in the deployed app:** Encoder and Frame rate menus open; picking 60 fps updated the field and Save wrote `fps = 60` to `~/.config/uscreen/config.toml`; restored to 90 and Saved; the file is byte-identical to the backup taken first.

## Other changes
- Floating labels removed from Select and TextInput (row label on the left only).
- Quality: CRF 12-32 in steps of 1, dotted track, live text next to the label: 12-15 Maximum sharpness, 16-20 Balanced, 21-25 Efficient, 26+ Low bandwidth. Bitrate: 5-60 Mbps, step 1, dotted track, value text next to the label. Ticks are the library's `showTicks` (on the track, not under it).
- The library's selected menu item used `tertiary-container`, which rendered cyan; the tertiary role in `theme.css` moved to a sea-green.
- Host daemon block is a bordered `surface-container` card; "Add device" is inset by the same padding so both buttons still share edges.
- Toast offset `bottom: 64`; verified it sits above the status bar.
- App locking moved to a new **Advanced** tab (Ctrl+2): switch, then an input with Add (Enter also adds), chips in a vertical list with remove, duplicate and invalid-ID messages. `locked_apps` is an array end to end. The per-device override on a device page uses the same list.

## Not verified
Dark mode was not re-screenshotted this round. The per-device App locking list was not exercised (no device attached). Tick density on the 56-step bitrate slider is busy by design of the step size.
