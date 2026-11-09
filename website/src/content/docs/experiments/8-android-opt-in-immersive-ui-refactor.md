---
title: "8 Android Opt-In Immersive UI Refactor"
sidebar:
  order: 7
  label: "8 Android Opt-In Immersive UI Refactor"
---

Following user feedback on the previous Android client build:
> *"The previous Android UI update was too aggressive. The custom top-edge swipe and 3-finger tap gestures actively interfere with the Samsung tablet's native Quick Access panel and other system gestures."*

| Field | Value |
| :-- | :-- |
| **ID** | 8 |
| **Date** | not recorded |
| **Build** | not recorded |
| **Verdict** | not recorded |
| **Raw results** | not recorded |

## What was tested

### Executive Summary & Problem Context

Following user feedback on the previous Android client build:
> *"The previous Android UI update was too aggressive. The custom top-edge swipe and 3-finger tap gestures actively interfere with the Samsung tablet's native Quick Access panel and other system gestures."*

On Samsung Galaxy tablets (e.g. Galaxy Tab S9 FE+, One UI / Android 16), swiping from the top screen bezel is natively bound to the system Quick Settings panel and Notification Shade, while multi-touch gestures interact with One UI taskbar/window management. Intercepting these events in the app layer broke system-level navigation.

### Directive Executed:

1. **Pristine Passthrough**: Completely reverted all custom gesture interceptors (top-edge swipe, 3-finger tap) in `TouchCapture.kt` to ensure 100% pure touch and S-Pen event forwarding to the Linux host with zero interference with Android/One UI system gestures.
2. **Strictly Opt-In Immersive Mode**: Implemented an explicit user toggle in the Settings sheet: `"Enable Immersive Mode (Hide Settings Button)"`, persisted across app launches via Android `SharedPreferences`.
3. **When OFF (Default)**: UI behaves **identically to the original APK** (Settings gear icon `⚙` visible at top right, standard cutout layout, standard Android Back button behavior, "Exit UScreen" button removed).
4. **When ON**: Settings button hidden during streaming, edge-to-edge cutout expansion enabled, Back button mapped via Compose `BackHandler` to toggle the Settings sheet open/closed, and the "Exit UScreen" button made available in the sheet.

---

### Task 0: Git Version Control

- Branch created and checked out: `feature/opt-in-immersive-ui`
- Base commit: `adc91ac766d5827b9365c96b4d82e3207057d946`

---

### Task 1: Code Cleanup & Persistent State Management



### `TouchCapture.kt` Cleanup

All gesture detection state (`onRequestSettings`, `edgeSwipeActive`, `edgeSwipeStartY`, `edgeSwipeStartX`) and conditional intercept blocks in `MotionEvent.ACTION_DOWN`, `ACTION_POINTER_DOWN`, `ACTION_MOVE`, `ACTION_UP`, `ACTION_CANCEL` were completely removed.
- **Verification**: `git diff main android/app/src/main/java/com/uscreen/TouchCapture.kt` produces 0 lines of diff.

### `Prefs.kt` SharedPreferences Persistence

Added `immersiveModeEnabled` property in `Prefs.kt` with default `false`:
```kotlin
/**
 * Opt-in immersive mode: hides the settings handle, expands into cutout,
 * and maps the Android Back button to toggle the settings sheet.
 * Default is false (OFF), matching original APK behavior.
 */
var immersiveModeEnabled: Boolean
    get() = sp.getBoolean("immersive_mode_enabled", false)
    set(v) = sp.edit().putBoolean("immersive_mode_enabled", v).apply()
```

---


## Result

### Task 2: UI Logic Implementation



### Settings Sheet Toggle

Added the toggle switch inside `SettingsSheet` in `MainActivity.kt`:
```kotlin
Row(
    verticalAlignment = Alignment.CenterVertically,
    modifier = Modifier.fillMaxWidth()
) {
    Column(modifier = Modifier.weight(1f)) {
        Text("Enable Immersive Mode (Hide Settings Button)", fontSize = 14.sp, color = Color(0xFFB0B0C0))
        Text(
            "Hides the gear button and maps the Android Back button to open settings",
            fontSize = 11.sp,
            color = Color(0xFF6A6A7E)
        )
    }
    Spacer(Modifier.width(12.dp))
    Switch(
        checked = immersiveModeEnabled,
        onCheckedChange = onImmersiveModeEnabledChange,
        colors = SwitchDefaults.colors(checkedTrackColor = Accent)
    )
}
```

### Behavioral Verification Matrix

| Aspect | Toggle OFF (Default) | Toggle ON (Opt-In) |
|---|---|---|
| **Settings Gear Button (`⚙`)** | **Always Visible** (top-right corner, alpha 0.35f when connected) | **Hidden** during active video stream; visible on connection screen |
| **Cutout Layout** | `LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT` (Original APK letterboxing) | `LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS` / `SHORT_EDGES` (100% edge-to-edge) |
| **Android Back Button** | **Standard Android Back** (closes app / sends to home launcher) | Intercepted via `BackHandler` -> Toggles Settings Sheet open / closed |
| **Exit UScreen Button** | **Removed** from Settings sheet | **Visible** at bottom of Settings sheet |
| **System Gestures** | Pure native Android/Samsung behavior (swiping top pulls Quick Settings) | Pure native Android/Samsung behavior |

### Conditional UI Visibility Logic

```kotlin
val showSettingsHandle = !immersiveOn || !isConnected

if (showSettingsHandle) {
    Box(
        modifier = Modifier
            .align(Alignment.TopEnd)
            .padding(10.dp)
            .size(38.dp)
            .alpha(if (penOnly && blankInPenMode) 0.18f else if (isConnected) 0.35f else 0.9f)
            .clip(CircleShape)
            .background(Color(0xAA20202C))
            .clickable { showSettings = true },
        contentAlignment = Alignment.Center
    ) {
        Text("⚙", fontSize = 18.sp, color = Color.White)
    }
}
```

---

### Task 3: Build, Deploy & Hardware Verification



### Build Verification

```bash
cd android
./gradlew assembleDebug
```
**Output**:
```text
BUILD SUCCESSFUL in 6s
33 actionable tasks: 5 executed, 28 up-to-date
Artifact: android/app/build/outputs/apk/debug/app-debug.apk (8.5 MB)
```

### ADB Deploy

```bash
adb -s TABLET_SERIAL install -r android/app/build/outputs/apk/debug/app-debug.apk
```
**Output**:
```text
Performing Streamed Install
Success
```

### Runtime Test Results on Tablet (`TABLET_SERIAL`)

1. **Default State (OFF)**:
   - App launched with `immersive_mode_enabled = false`.
   - Settings button `⚙` is visible in the top-right corner over the live stream.
   - Pressed Android Back button (`input keyevent KEYCODE_BACK`): App exited to Samsung Launcher (`com.sec.android.app.launcher.activities.LauncherActivity`), demonstrating standard unmodified Back behavior.
   - Opened Settings sheet: Toggle displayed as OFF; "Exit UScreen" button was completely absent.
   - Screenshot captured: `experiments_and_findings/screenshots/08_default_state_off.png`, `08_settings_sheet_off.png`, `08_settings_bottom_off.png`.

2. **Opt-In State (ON)**:
   - Toggled "Enable Immersive Mode (Hide Settings Button)" to ON.
   - Settings button `⚙` immediately disappeared from the screen.
   - Stream expanded into 100% full screen.
   - Pressed Android Back button: Settings sheet smoothly slid open over the live video stream.
   - Scrolled to bottom: "Exit UScreen" button appeared in red outline below "Apply".
   - Pressed Android Back button again: Settings sheet smoothly dismissed back to full screen.
   - Swiped from top bezel (`input swipe 1280 0 1280 800 250`): Samsung system quick menu / taskbar opened with zero app interference.
   - Screenshot captured: `experiments_and_findings/screenshots/08_optin_state_on.png`, `08_settings_opened_by_back.png`, `08_settings_sheet_on.png`, `08_settings_bottom_on.png`, `08_settings_closed_by_back.png`, `08_samsung_system_menu.png`.

---

### Code Diffs



### `android/app/src/main/java/com/uscreen/Prefs.kt`

```diff
--- a/android/app/src/main/java/com/uscreen/Prefs.kt
+++ b/android/app/src/main/java/com/uscreen/Prefs.kt
@@ -61,6 +61,15 @@ class Prefs(context: Context) {
         get() = sp.getBoolean("touch_enabled", true)
         set(v) = sp.edit().putBoolean("touch_enabled", v).apply()
 
+    /**
+     * Opt-in immersive mode: hides the settings handle, expands into cutout,
+     * and maps the Android Back button to toggle the settings sheet.
+     * Default is false (OFF), matching original APK behavior.
+     */
+    var immersiveModeEnabled: Boolean
+        get() = sp.getBoolean("immersive_mode_enabled", false)
+        set(v) = sp.edit().putBoolean("immersive_mode_enabled", v).apply()
+
     /**
      * Which way round the tablet is held: [ORIENTATION_AUTO] follows the
      * sensor between the two landscape directions, the other two pin it.
```

### `android/app/src/main/java/com/uscreen/MainActivity.kt`

```diff
--- a/android/app/src/main/java/com/uscreen/MainActivity.kt
+++ b/android/app/src/main/java/com/uscreen/MainActivity.kt
@@ -9,6 +9,7 @@ import android.view.View
 import android.view.WindowInsetsController
 import android.view.WindowManager
 import androidx.activity.ComponentActivity
+import androidx.activity.compose.BackHandler
 import androidx.activity.compose.setContent
 import androidx.compose.animation.*
 import androidx.compose.foundation.background
@@ -156,7 +157,7 @@ class MainActivity : ComponentActivity() {
 
         // Enable fullscreen AFTER setContent so DecorView exists
         window.decorView.post {
-            enableImmersiveMode()
+            applyImmersiveMode()
         }
     }
 
@@ -248,15 +249,42 @@ class MainActivity : ComponentActivity() {
         tiltListener = null
     }
 
-    private fun enableImmersiveMode() {
+    override fun onResume() {
+        super.onResume()
+        applyImmersiveMode()
+    }
+
+    fun applyImmersiveMode(optIn: Boolean = prefs.immersiveModeEnabled) {
         try {
+            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
+                val lp = window.attributes
+                if (optIn) {
+                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
+                        lp.layoutInDisplayCutoutMode =
+                            WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
+                    } else {
+                        lp.layoutInDisplayCutoutMode =
+                            WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
+                    }
+                } else {
+                    lp.layoutInDisplayCutoutMode =
+                        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT
+                }
+                window.attributes = lp
+            }
+
             if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                 window.setDecorFitsSystemWindows(false)
                 window.insetsController?.let { controller ->
-                    controller.hide(
-                        android.view.WindowInsets.Type.statusBars() or
-                        android.view.WindowInsets.Type.navigationBars()
-                    )
+                    if (optIn) {
+                        controller.hide(android.view.WindowInsets.Type.systemBars())
+                    } else {
+                        controller.hide(
+                            android.view.WindowInsets.Type.statusBars() or
+                            android.view.WindowInsets.Type.navigationBars()
+                        )
+                    }
                     controller.systemBarsBehavior =
                         WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                 }
@@ -335,7 +363,7 @@ class MainActivity : ComponentActivity() {
     override fun onWindowFocusChanged(hasFocus: Boolean) {
         super.onWindowFocusChanged(hasFocus)
         if (hasFocus) {
-            enableImmersiveMode()
+            applyImmersiveMode()
             requestHighestRefreshRate()
         }
     }
@@ -410,14 +438,28 @@ fun UScreenMain(
     var isConnected by remember { mutableStateOf(false) }
     var fps by remember { mutableStateOf(0f) }
     var mbps by remember { mutableStateOf(0f) }
-    var showOverlay by remember { mutableStateOf(true) }
     var showSettings by remember { mutableStateOf(false) }
     var showStats by remember { mutableStateOf(prefs?.showStats ?: false) }
     var touchOn by remember { mutableStateOf(prefs?.touchEnabled ?: true) }
     var blankInPenMode by remember { mutableStateOf(prefs?.blankPenOnly ?: true) }
     var orientationChoice by remember { mutableStateOf(prefs?.orientation ?: Prefs.ORIENTATION_AUTO) }
+    var immersiveOn by remember { mutableStateOf(prefs?.immersiveModeEnabled ?: false) }
 
     val context = LocalContext.current
+    val activity = context as? MainActivity
+
+    // Only intercept Back button when opt-in immersive mode is ON and streaming or pen mode.
+    // When immersive mode is OFF (Default), Back button behaves as normal Android back (does nothing special).
+    BackHandler(enabled = immersiveOn && (isConnected || penOnly) && !showSettings) {
+        showSettings = true
+    }
+
+    // Re-assert immersive mode whenever settings sheet closes
+    LaunchedEffect(showSettings) {
+        if (!showSettings) {
+            activity?.applyImmersiveMode(immersiveOn)
+        }
+    }
@@ -521,6 +553,10 @@ fun UScreenMain(
-        // Subtle settings handle (top-right). Sits above the video surface, so
-        // taps here are NOT forwarded to the Linux host.
-        Box(
+        // Settings handle (top-right).
+        // OFF (Default): Always visible, exactly matching the original APK.
+        // ON (Opt-in): Hidden during streaming, visible when disconnected.
+        val showSettingsHandle = !immersiveOn || !isConnected
+
+        if (showSettingsHandle) {
+            Box(
                 modifier = Modifier
                     .align(Alignment.TopEnd)
                     .padding(10.dp)
@@ -569,7 +605,7 @@ fun UScreenMain(
-        if (updateAvailable != null) {
+        if (updateAvailable != null && showSettingsHandle) {
             Box(
@@ -637,6 +672,15 @@ fun UScreenMain(
                 onApply = { bitrateKbps, newFps ->
                     prefs?.bitrateKbps = bitrateKbps
                     prefs?.fps = newFps
                     touchCapture?.sendConfig(bitrateKbps, newFps)
                 },
+                immersiveModeEnabled = immersiveOn,
+                onImmersiveModeEnabledChange = {
+                    immersiveOn = it
+                    prefs?.immersiveModeEnabled = it
+                    activity?.applyImmersiveMode(it)
+                },
+                onExitApp = {
+                    activity?.finish()
+                },
                 onDismiss = { showSettings = false }
             )
         }
@@ -775,6 +819,9 @@ private fun SettingsSheet(
     orientation: Int,
     onOrientationChange: (Int) -> Unit,
     onApply: (bitrateKbps: Int, fps: Int) -> Unit,
+    immersiveModeEnabled: Boolean = false,
+    onImmersiveModeEnabledChange: (Boolean) -> Unit = {},
+    onExitApp: () -> Unit = {},
     onDismiss: () -> Unit,
 ) {
@@ -1007,6 +1054,30 @@ private fun SettingsSheet(
+            // Opt-in immersive mode toggle.
+            // OFF (Default): original APK UI with visible settings button and standard Back button behavior.
+            // ON: full-screen immersive with hidden settings button and Back button mapping.
+            Row(
+                verticalAlignment = Alignment.CenterVertically,
+                modifier = Modifier.fillMaxWidth()
+            ) {
+                Column(modifier = Modifier.weight(1f)) {
+                    Text("Enable Immersive Mode (Hide Settings Button)", fontSize = 14.sp, color = Color(0xFFB0B0C0))
+                    Text(
+                        "Hides the gear button and maps the Android Back button to open settings",
+                        fontSize = 11.sp,
+                        color = Color(0xFF6A6A7E)
+                    )
+                }
+                Spacer(Modifier.width(12.dp))
+                Switch(
+                    checked = immersiveModeEnabled,
+                    onCheckedChange = onImmersiveModeEnabledChange,
+                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
+                )
+            }
+            Spacer(Modifier.height(24.dp))
+
             if (!penOnly) {
                 Button(
                     onClick = {
@@ -1029,6 +1100,17 @@ private fun SettingsSheet(
                     modifier = Modifier.fillMaxWidth()
                 )
             }
+
+            if (immersiveModeEnabled) {
+                Spacer(Modifier.height(16.dp))
+                OutlinedButton(
+                    onClick = onExitApp,
+                    modifier = Modifier.fillMaxWidth(),
+                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color(0xFFFF6B6B))
+                ) {
+                    Text("Exit UScreen", fontSize = 14.sp)
+                }
+            }
             Spacer(Modifier.height(24.dp))
```


## Not verified

The write-up lists nothing as not verified. That is not a claim that everything was.

## Sources

- [`experiments_and_findings/08_android_opt_in_immersive.md`](https://github.com/harshp2008/UScreen-Pro/blob/main/experiments_and_findings/08_android_opt_in_immersive.md)
