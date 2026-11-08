# Experiment & Finding 07: Android Immersive UI & Settings Overlay Concealment

## Overview
- **Device Tested**: Samsung Galaxy Tab S9 FE+ (`SM-X616B`, Device ID: `TABLET_SERIAL`), Android 16
- **Physical Panel Resolution**: 2560 x 1600 @ 90Hz
- **Issue Addressed**: The UScreen tablet client previously rendered a persistent settings button (`⚙`) in the top-right corner and potential update badges directly over the active video stream. In secondary monitor operation, this permanently consumed physical screen real estate, blocked view of host UI elements (e.g., window close buttons / system tray in the top-right corner), and intercepted pointer clicks in that region.
- **Objective**: Hide all Settings UI elements by default during active streaming to provide a 100% full-screen immersive display, and implement intuitive gestures and Back-button interactions to reveal and dismiss the Settings UI without disrupting the video stream, touch forwarding, or S-Pen drawing.

---

## Task 1: Codebase Investigation Findings

1. **Architecture & UI Framework**:
   - **Language**: Kotlin (`JVM 17`, Kotlin 1.9.20)
   - **UI Framework**: Modern Jetpack Compose with Material 3 (`androidx.compose:compose-bom:2024.02.02`, `material3:1.2.1`, `activity-compose:1.8.2`).
   - **Video Rendering Surface**: `SurfaceView` wrapped within Compose via `AndroidView` in `MainActivity.kt`. Configured with `PixelFormat.OPAQUE` and hooked directly into `VideoReceiver` (MediaCodec hardware decoder) and `TouchCapture` (WebSocket input pipeline).
   - **Entry Point**: `com.uscreen.MainActivity` (`ComponentActivity`).

2. **Root Cause Analysis of Persistent Settings Handle**:
   - In `MainActivity.kt` lines 521-536, the ⚙ button was an unconditional `Box` placed at `Alignment.TopEnd` of the root Compose `Box`.
   - Although a state variable `showOverlay` existed, it was never connected to the settings button's visibility. The 38dp button + 10dp padding remained permanently visible, overlaying the video stream and consuming touch clicks with `clickable { showSettings = true }`.

---

## Task 2: Implementation Details

### 1. True Edge-to-Edge Immersive Mode
- **Cutout Handling**: Configured `layoutInDisplayCutoutMode = LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS` (API 30+) / `LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES` (API 28+). This prevents tablet letterboxing around the front camera cutout, extending the video surface into 100% of the 2560x1600 panel.
- **System Bars**: Enhanced `enableImmersiveMode()` to hide `WindowInsets.Type.systemBars()` (status bars, navigation bars, caption bars) with `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE`.
- **Lifecycle Guarantees**: Re-asserted `enableImmersiveMode()` in `onResume()`, `onWindowFocusChanged(hasFocus = true)`, and immediately whenever the `ModalBottomSheet` dismisses.

### 2. Default-Hidden Settings UI Overlay
- Replaced the persistent `Box` in `UScreenMain` with `AnimatedVisibility(visible = !isConnected || showOverlay)`.
- When streaming (`isConnected == true`), the settings handle and update banner are hidden by default (`0dp` occupied, `0%` obstruction).
- When disconnected (`ConnectionScreen`), the button remains visible so users can adjust settings before establishing a connection.

### 3. Reveal & Dismiss Interaction Mechanisms
Implemented multiple interaction methods:
1. **Universal Android Back Navigation (`BackHandler`)**:
   - When streaming and Settings is closed: Pressing the Android Back button (hardware key, virtual nav bar button, keyboard ESC, or Android system edge-back swipe from left/right) sets `showSettings = true`, smoothly sliding open `ModalBottomSheet`.
   - When Settings is open: Pressing Back again dismisses the sheet back to the 100% full-screen video stream.
   - When tapping outside the sheet (scrim): The sheet dismisses back to the full-screen stream.
2. **Top-Edge Swipe Down Gesture**:
   - Added finger swipe detection in `TouchCapture.kt`. If a finger touch initiates near the top screen boundary (`y < 80px` or `< 6%` of display height) and moves downward (`dy > 100px` with vertical dominance `dy > dx * 1.3`), `onRequestSettings` is invoked.
   - The touch slot is lifted so no dragging event is mistakenly sent to the Linux host.
3. **Multi-Touch 3-Finger Tap**:
   - Tapping with 3 fingers simultaneously on the screen triggers `onRequestSettings`.
4. **Transient System Bar Reveal**:
   - Added `setOnApplyWindowInsetsListener` on the DecorView. Swiping to reveal transient system bars triggers `showOverlay = true` for 4 seconds before automatically fading out.
5. **Direct Exit Button**:
   - Added an explicit "Exit UScreen" button inside `SettingsSheet` for clean app exit without guesswork.

### 4. Preservation of Video, Touch, and Stylus Pipelines
- S-Pen stylus events are checked via `isPenLike()` and are strictly bypassed from all gesture triggers. Pen hover, stroke drawing, pressure, tilt, and eraser functionality operate with zero latency or obstruction.
- When Settings is hidden, no Compose composable overlays the `SurfaceView` at `Alignment.TopEnd`, allowing host UI controls (like the window close button) to be clicked directly.
- Video decoding via `MediaCodec` continues uninterrupted while settings are revealed or dismissed.

---

## Task 3: Build, Deploy & Verification

### Exact Commands Executed

1. **Gradle Build**:
   ```bash
   cd android
   ./gradlew assembleDebug
   ```
   - **Result**: `BUILD SUCCESSFUL in 13s`
   - **Artifact**: `android/app/build/outputs/apk/debug/app-debug.apk` (8.5 MB)

2. **ADB Installation on Tablet**:
   ```bash
   adb -s TABLET_SERIAL uninstall com.uscreen
   adb -s TABLET_SERIAL install -r android/app/build/outputs/apk/debug/app-debug.apk
   ```
   - **Result**: `Performing Streamed Install` -> `Success`

3. **App Launch & Verification**:
   ```bash
   adb -s TABLET_SERIAL shell am start -n com.uscreen/.MainActivity
   ```
   - **Result**: `Starting: Intent { cmp=com.uscreen/.MainActivity }`

4. **Runtime Verification**:
   - `dumpsys window windows`:
     - `mAttrs={(0,0)(fillxfill) layoutInDisplayCutoutMode=always bhv=2}`
     - `Frames: parent=[0,0][2560,1600] display=[0,0][2560,1600] frame=[0,0][2560,1600]`
   - Screencap Verification:
     - Normal streaming: 100% full screen video stream, zero overlay buttons or bars.
     - Back button press (`input keyevent KEYCODE_BACK`): Settings sheet slides up over live stream.
     - Second Back button press: Settings sheet dismisses, full screen restored.
     - Tap outside sheet (`input tap 1280 200`): Settings sheet dismisses.
     - Top-edge swipe down (`input swipe 1280 20 1280 300 200`): Settings sheet slides up.
