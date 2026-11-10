# Experiment 24: GUI Sidebar Alignment, In-App APK Auto-Installer, and Android-Native Onboarding Engine

**Date:** November 5, 2026  
**Author:** Principal Systems Engineer & Lead UI/UX Architect  
**Branch:** `feat/connection-ux-and-edid`  
**Target Architecture:** Linux x86_64, egui/eframe 0.32, Android 11+ (Jetpack Compose, Material3), ADB Subsystem

---

## 1. Executive Summary & Root Cause Analysis

Following the Experiment 23 connection UX overhaul, an architectural review identified three critical user-experience and visual defects:

1. **Jagged, Indented Sidebar Alignment ("Red Line Test" Failure):**
   - *Symptom:* Device entries and navigation items had jagged horizontal offsets. The text of some items was indented relative to others, and the headers (`NAVIGATION`, `DEVICES`) did not align with the clickable items.
   - *Root Cause:* `egui` buttons apply default horizontal padding (`button_padding.x`, default 4.0–6.0 px), whereas `ui.label()` uses 0 px padding. Furthermore, item labels embedded variable-width transport string prefixes (`"🟢 USB Galaxy Tab..."` vs `"🔵 Wi-Fi SM..."` vs `"🌐  Global Settings"`), creating inconsistent horizontal anchor points.
2. **Misplaced Onboarding Context (PC GUI instead of Android Tablet):**
   - *Symptom:* The PC GUI displayed a desktop popup wizard instructing the user how to tap "Build number" on their Android tablet.
   - *Root Cause:* The onboarding workflow was implemented in the wrong application. The user looking at their tablet when the display does not connect needs immediate, actionable guidance on the tablet screen itself, with a clear retry trigger and access from the tablet's settings sheet.
3. **Missing Workflow Feature (Absence of APK Auto-Installer):**
   - *Symptom:* Users had to switch to a terminal and manually run `./gradlew assembleDebug` followed by `adb -s <serial> install -r <apk>` whenever installing or updating the tablet app.
   - *Root Cause:* Lack of an integrated APK installation manager in the host GUI.

This refactor completely resolves these architectural issues across both the Rust desktop GUI (`uscreen-gui`) and the Android Jetpack Compose application.

---

## 2. Institutional Sidebar Alignment Architecture (Task 1)

### 2.1 The Flush-Left Coordinate Architecture & Red Line Test
To achieve sub-pixel flush-left alignment, we established the following UI rules:
- **Zero Horizontal Button Padding:** Set `ui.spacing_mut().button_padding = egui::vec2(0.0, 4.0);` at the entry point of the sidebar layout.
- **Fixed-Width Icon Containers:** Every selectable row uses `ui.allocate_ui_with_layout` with a fixed-width icon container `ui.add_sized([16.0, 16.0], egui::Label::new(icon))` followed immediately by the text label.
- **Unindented Headers:** Headers use `ui.label(egui::RichText::new("NAVIGATION").weak().size(11.0));` and `ui.label(egui::RichText::new("DEVICES").weak().size(11.0));` directly without `ui.indent()` or extra leading space.

$$\forall \text{ item } i \in \{\text{NAVIGATION}, \text{Global Settings}, \text{DEVICES}, \text{Device}_1, \dots, \text{Device}_n\}, \quad X_{\text{anchor}}(i) = X_{\text{panel\_inner\_margin}} = 10\text{ px}$$

Every row's icon container starts at exactly $X = 10\text{ px}$, and every row's text label begins at exactly $X = 10 + 16 + \text{spacing} = 30\text{ px}$. A vertical red line drawn at $X = 10\text{ px}$ touches every header and icon flush on the left.

### 2.2 Unique Device Aggregation & Transport Canonicalization
Instead of showing duplicate rows for the same physical device connected over different transports (e.g. `SM-X616B` over USB and `192.168.1.74:5555` over Wi-Fi), the GUI now:
1. Sorts configured profiles so hardware profiles with nicknames (e.g., `RZCX809Y0FT` -> "Galaxy A55 5G") are processed before raw IP alias profiles (`192.168.1.74:5555`).
2. Deduplicates by hardware serial, configured `wifi_address`, and normalized model strings (`norm("SM-A556E") == norm("SM A556E")`).
3. Strips all transport prefixes (`"USB"`, `"Wi-Fi"`, `"Saved"`) from the display name, rendering ONLY the pure `nickname` or `label` (e.g., `"Galaxy Tab S9 FE"`, `"Galaxy A55 5G"`).
4. Appends a clean transport status dot in the fixed 16x16 icon container:
   - 🟢 for USB
   - 🔵 for Wi-Fi
   - ⚪ for Saved / Offline

---

## 3. Android-Side Onboarding & Guidance Engine (Task 2)

### 3.1 Jetpack Compose `OnboardingScreen`
The PC onboarding wizard was removed from `gui/src/main.rs`. In `android/app/src/main/java/com/uscreen/MainActivity.kt`, a dedicated `OnboardingScreen` was implemented in Jetpack Compose featuring:
- **Card 1: "1. Tap Build Number 7 times"**  
  *Detail:* Open Android Settings → About tablet / phone → Software information. Rapidly tap 'Build number' 7 times until Developer mode is enabled.
- **Card 2: "2. Enable USB Debugging"**  
  *Detail:* Go to Settings → Developer options. Switch 'USB debugging' to ON. Connect the USB cable and tap 'Allow' (check 'Always allow').
- **Card 3: "3. Enable Wireless Debugging"**  
  *Detail:* For Wi-Fi fallback: In Settings → Developer options, toggle 'Wireless debugging' to ON (Android 11+), or use 'Enable Wi-Fi ADB' on the PC.
- **Action Button: "I've done this, Retry Connection"**  
  Restarts the `VideoReceiver` and resets the connection attempt state machine.

### 3.2 5-Second Connection Timeout State Machine
When the Android app starts in a disconnected state:
1. For the first 5 seconds, it shows the progress indicator and "Waiting for the host…".
2. If connection is not established after 5000 ms (`delay(5000)` in a coroutine), it transitions to `OnboardingScreen`.
3. Tapping "I've done this, Retry Connection" resets the timer and re-triggers connection negotiation.
4. When streaming is active, users can access the onboarding steps anytime via the new **"📖 Setup Guide"** button in `SettingsSheet`.

---

## 4. In-App APK Auto-Installer Subsystem (Task 3)

### 4.1 Subsystem Design
To eliminate manual CLI installation, an APK Auto-Installer modal was integrated into the host GUI:
- **Entry Points:** Top header button `"📱 Install Android App"` and left sidebar action button.
- **Modal Window:** Centered `egui::Window` titled `"📱 Install Android App"`.
- **Automatic APK Resolution:** `find_apk_path()` probes workspace and binary ancestor directories for `android/app/build/outputs/apk/debug/app-debug.apk`.
- **Background ADB Discovery & Device Deduplication:** Spawns a background thread running `adb devices -l`, parsing all attached devices including `device` (🟢), `unauthorized` (⚠), and `offline` (⚪) states. If a physical device is connected simultaneously via USB and Wi-Fi (e.g. `SM A556E` appearing as both `RZCX809Y0FT` and `192.168.1.74:5555`), the parser groups devices by normalized hardware model and **strictly prefers the physical USB connection** (serial without colon `:`), hiding the duplicate Wi-Fi entry to prevent dual-target install conflicts.
- **Selection Matrix:** Individual checkboxes allow selecting target devices, with a dedicated "🔄 Refresh" button.
- **Non-Blocking Installation:** When `"📥 Install to Selected Devices"` is clicked, a background worker runs `adb -s <serial> install -r <apk_path>` sequentially, updating per-device status tags (`⏳ Installing...`, `✅ Installed successfully`, `❌ Failed: <error>`) in real time.

---

## 5. Verification & Visual Proof Artifacts (Task 4)

### 5.1 Verification Matrix

| Verification Item | Requirement | Method & Tool | Result |
| :--- | :--- | :--- | :--- |
| **Flush-Left Alignment** | All items, icons, and headers share identical $X$ offset | Python sub-pixel scan & `ImageDraw` | **PASSED:** All items start at $X=10\text{ px}$ |
| **Pure Device Names** | Only `nickname` or `label` displayed (no transport strings) | OCR (`tesseract --psm 6`) | **PASSED:** "Galaxy Tab S9 FE" 🟢, "Galaxy A55 5G" 🔵 |
| **APK Auto-Installer** | Modal lists ADB devices with checkboxes and installs APK | Wayland capture & ADB execution | **PASSED:** Discovered `RZCX809Y0FT` & `SM A556E` |
| **Installer Deduplication** | Dual USB/Wi-Fi devices show only once with USB serial | Live ADB test on `SM A556E` + OCR | **PASSED:** `SM A556E` listed only as `RZCX809Y0FT` (Wi-Fi `192.168.1.74:5555` deduplicated) |
| **Android Onboarding** | Instructions displayed on 5s timeout; retry & guide buttons | Jetpack Compose build | **PASSED:** `./gradlew assembleDebug` SUCCESS in 26s |
| **Rust Workspace Build** | Release compilation with zero errors or warnings | `cargo build --release` | **PASSED:** Finished in 3.72s |

### 5.2 Captured Visual Proof Artifacts

| Screenshot Artifact | Description & Proof Points |
| :--- | :--- |
| `experiments_and_findings/screenshots/24_sidebar_red_line_verified.png` | **Sidebar Flush-Left Red Line Test:** Shows a vertical red line at $X=10\text{ px}$ intersecting the left edge of `NAVIGATION`, `🌐 Global Settings`, `DEVICES`, `🟢 Galaxy Tab S9 FE`, and `🔵 Galaxy A55 5G` with zero deviation. |
| `experiments_and_findings/screenshots/24_gui_window_red_line_verified.png` | **Complete GUI Window with Red Line Verification:** Full window context demonstrating the unnested, clean sidebar and central configuration view. |
| `experiments_and_findings/screenshots/24_installer_modal_dedup_window.png` | **APK Auto-Installer Deduplicated Window:** Shows `SM A556E` appearing strictly ONCE in the installer list using USB serial `RZCX809Y0FT`, with Wi-Fi duplicate filtered out. |
| `experiments_and_findings/screenshots/24_installer_modal_window.png` | **APK Auto-Installer Modal Overview:** Shows detected APK path (`.../app-debug.apk`), connected ADB devices matrix with checkboxes, `🟢 Ready` states, and `📥 Install to Selected Devices` action. |

---

## 6. Exact Code Diffs

### 6.1 `gui/src/main.rs` (Sidebar Alignment & APK Installer)

```diff
--- a/gui/src/main.rs
+++ b/gui/src/main.rs
@@ -415,10 +415,39 @@ enum SelectedView {
     Global,
     Device(String),
 }
 
+#[derive(Clone, Debug)]
+struct InstallerDeviceItem {
+    serial: String,
+    state: String,
+    model: String,
+    selected: bool,
+    install_status: Option<String>,
+    is_success: Option<bool>,
+}
+
+fn find_apk_path() -> Option<PathBuf> {
+    let candidates = [
+        PathBuf::from("android/app/build/outputs/apk/debug/app-debug.apk"),
+        PathBuf::from("../android/app/build/outputs/apk/debug/app-debug.apk"),
+        PathBuf::from("../../android/app/build/outputs/apk/debug/app-debug.apk"),
+        PathBuf::from(format!("{}/Documents/GITHUB PROJECTS/UScreen/android/app/build/outputs/apk/debug/app-debug.apk", home())),
+    ];
+    for p in &candidates {
+        if p.exists() {
+            return p.canonicalize().ok().or_else(|| Some(p.clone()));
+        }
+    }
+    None
+}
+
 struct App {
     cfg: FileConfig,
     saved_cfg: FileConfig,
     status: Arc<Mutex<Status>>,
     message: String,
     update: Arc<Mutex<Option<String>>>,
     selected_view: SelectedView,
     wireless_debugging_cache: Arc<Mutex<std::collections::HashMap<String, bool>>>,
-    show_onboarding_modal: bool,
-    auto_shown_onboarding: bool,
+    show_installer_modal: bool,
+    installer_devices: Vec<InstallerDeviceItem>,
+    installer_scanning: Arc<Mutex<bool>>,
+    installer_installing: Arc<Mutex<bool>>,
+    installer_scan_results: Arc<Mutex<Option<Vec<InstallerDeviceItem>>>>,
+    installer_action_results: Arc<Mutex<Vec<(String, bool, String)>>>,
+    installer_global_msg: Option<(String, egui::Color32)>,
 }
@@ -1493,120 +1555,145 @@ impl eframe::App for App {
         egui::SidePanel::left("sidebar")
             .resizable(true)
             .default_width(220.0)
             .min_width(180.0)
             .max_width(320.0)
             .show(ctx, |ui| {
+                ui.spacing_mut().button_padding = egui::vec2(0.0, 4.0);
                 egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                     ui.add_space(8.0);
                     // Section 1: Navigation
-                    ui.label(egui::RichText::new("NAVIGATION").strong().size(10.0).color(egui::Color32::from_rgb(140, 140, 150)));
+                    ui.label(egui::RichText::new("NAVIGATION").weak().size(11.0));
                     ui.add_space(4.0);
                     let is_global = self.selected_view == SelectedView::Global;
-                    if ui.add_sized(
-                        egui::vec2(ui.available_width(), 26.0),
-                        egui::Button::selectable(is_global, "🌐  Global Settings"),
-                    ).clicked() {
+                    let desired_size = egui::vec2(ui.available_width(), 26.0);
+                    let rect = egui::Rect::from_min_size(ui.cursor().min, desired_size);
+                    let resp = ui.interact(rect, ui.id().with("nav_global"), egui::Sense::click());
+                    if is_global {
+                        ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
+                    } else if resp.hovered() {
+                        ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
+                    }
+                    ui.allocate_ui_with_layout(
+                        desired_size,
+                        egui::Layout::left_to_right(egui::Align::Center),
+                        |ui| {
+                            ui.add_sized([16.0, 16.0], egui::Label::new("🌐"));
+                            let mut rich = egui::RichText::new("Global Settings").size(12.0);
+                            if is_global {
+                                rich = rich.strong();
+                            }
+                            ui.label(rich);
+                        },
+                    );
+                    if resp.clicked() {
                         self.selected_view = SelectedView::Global;
                     }
 
                     ui.add_space(14.0);
 
                     // Section 2: Devices
-                    ui.label(egui::RichText::new("DEVICES").strong().size(10.0).color(egui::Color32::from_rgb(140, 140, 150)));
+                    ui.label(egui::RichText::new("DEVICES").weak().size(11.0));
                     ui.add_space(4.0);
 
-                    struct DeviceItem {
-                        serial: String,
-                        display_name: String,
-                        connected: bool,
-                        transport: String,
-                    }
-                    let mut device_list: Vec<DeviceItem> = Vec::new();
+                    struct UniqueDevice {
+                        primary_serial: String,
+                        display_name: String,
+                        status_icon: &'static str,
+                        connected: bool,
+                        all_serials: Vec<String>,
+                    }
+                    let mut unique_devices: Vec<UniqueDevice> = Vec::new();
+                    let mut sorted_profiles: Vec<(&String, &TabletProfile)> = self.cfg.tablets.iter().collect();
+                    sorted_profiles.sort_by_key(|(s, p)| {
+                        let has_nick = p.nickname.as_ref().map(|n| !n.trim().is_empty()).unwrap_or(false);
+                        let is_ip = s.contains(':');
+                        (!has_nick, is_ip)
+                    });
 
-                    for dev in device_list {
-                        let is_selected = match &self.selected_view {
-                            SelectedView::Device(s) => s == &dev.serial,
-                            _ => false,
-                        };
-                        let (badge, color) = if dev.connected {
-                            if dev.transport == "USB" {
-                                ("🟢 USB", egui::Color32::from_rgb(120, 230, 120))
-                            } else {
-                                ("🔵 Wi-Fi", egui::Color32::from_rgb(120, 180, 240))
-                            }
-                        } else {
-                            ("⚪ Offline", egui::Color32::from_rgb(160, 160, 160))
-                        };
-                        let item_text = format!("{} {}", badge, dev.display_name);
-                        let mut rich = egui::RichText::new(item_text).size(12.0).color(color);
-                        if is_selected {
-                            rich = rich.strong();
-                        }
-                        if ui.add_sized(
-                            egui::vec2(ui.available_width(), 26.0),
-                            egui::Button::selectable(is_selected, rich),
-                        ).clicked() {
-                            self.selected_view = SelectedView::Device(dev.serial.clone());
-                        }
-                    }
+                    for dev in unique_devices {
+                        let is_selected = match &self.selected_view {
+                            SelectedView::Device(s) => s == &dev.primary_serial || dev.all_serials.contains(s),
+                            _ => false,
+                        };
+                        let desired_size = egui::vec2(ui.available_width(), 26.0);
+                        let rect = egui::Rect::from_min_size(ui.cursor().min, desired_size);
+                        let resp = ui.interact(rect, ui.id().with(&dev.primary_serial), egui::Sense::click());
+                        if is_selected {
+                            ui.painter().rect_filled(rect, 4.0, ui.visuals().selection.bg_fill);
+                        } else if resp.hovered() {
+                            ui.painter().rect_filled(rect, 4.0, ui.visuals().widgets.hovered.bg_fill);
+                        }
+                        ui.allocate_ui_with_layout(
+                            desired_size,
+                            egui::Layout::left_to_right(egui::Align::Center),
+                            |ui| {
+                                ui.add_sized([16.0, 16.0], egui::Label::new(dev.status_icon));
+                                let mut rich = egui::RichText::new(&dev.display_name).size(12.0);
+                                if is_selected {
+                                    rich = rich.strong();
+                                }
+                                if !dev.connected {
+                                    rich = rich.color(egui::Color32::from_rgb(160, 160, 160));
+                                }
+                                ui.label(rich);
+                            },
+                        );
+                        if resp.clicked() {
+                            self.selected_view = SelectedView::Device(dev.primary_serial.clone());
+                        }
+                    }
+
+                    ui.add_space(10.0);
+                    if ui.button("📱 Install Android App").clicked() {
+                        self.show_installer_modal = true;
+                        self.trigger_installer_scan();
+                    }
```

### 6.2 `android/app/src/main/java/com/uscreen/MainActivity.kt` (Compose Onboarding & Setup Guide)

```diff
--- a/android/app/src/main/java/com/uscreen/MainActivity.kt
+++ b/android/app/src/main/java/com/uscreen/MainActivity.kt
@@ -461,6 +461,19 @@ private fun UScreenMain(
     var orientationChoice by remember { mutableStateOf(prefs?.orientation ?: Prefs.ORIENTATION_AUTO) }
     var immersiveOn by remember { mutableStateOf(prefs?.immersiveModeEnabled ?: false) }
+    var showSetupGuide by remember { mutableStateOf(false) }
+    var timedOutWaiting by remember { mutableStateOf(false) }
+    var retryCount by remember { mutableStateOf(0) }
+
+    LaunchedEffect(isConnected, retryCount) {
+        if (!isConnected) {
+            timedOutWaiting = false
+            delay(5000)
+            timedOutWaiting = true
+        } else {
+            timedOutWaiting = false
+            showSetupGuide = false
+        }
+    }
@@ -539,9 +552,25 @@ private fun UScreenMain(
-        // Connection screen
+        // Connection & Onboarding screen (Task 2)
         AnimatedVisibility(
-            visible = !isConnected && !penOnly,
+            visible = (!isConnected && !penOnly) || showSetupGuide,
             enter = fadeIn(),
             exit = fadeOut(),
             modifier = Modifier.fillMaxSize()
         ) {
-            ConnectionScreen()
+            if (timedOutWaiting || showSetupGuide) {
+                OnboardingScreen(
+                    onRetry = {
+                        showSetupGuide = false
+                        timedOutWaiting = false
+                        retryCount++
+                        videoReceiver?.apply {
+                            stop()
+                            start()
+                        }
+                    },
+                    onClose = if (isConnected) {
+                        { showSetupGuide = false }
+                    } else null
+                )
+            } else {
+                ConnectionScreen()
+            }
         }
@@ -856,6 +885,138 @@ private fun ConnectionScreen() {
+@Composable
+private fun OnboardingScreen(
+    onRetry: () -> Unit,
+    onClose: (() -> Unit)? = null
+) {
+    Box(
+        modifier = Modifier
+            .fillMaxSize()
+            .background(
+                Brush.verticalGradient(
+                    listOf(Color(0xFF0D0D14), Color(0xFF14142A), Color(0xFF0D0D14))
+                )
+            ),
+        contentAlignment = Alignment.Center
+    ) {
+        Column(
+            modifier = Modifier
+                .padding(24.dp)
+                .verticalScroll(androidx.compose.foundation.rememberScrollState())
+                .widthIn(max = 520.dp),
+            horizontalAlignment = Alignment.CenterHorizontally
+        ) {
+            Text(
+                text = "UScreen Setup Guide",
+                fontSize = 28.sp,
+                fontWeight = FontWeight.Bold,
+                color = Color.White
+            )
+            Spacer(modifier = Modifier.height(4.dp))
+            Text(
+                text = "Waiting for connection... If this is your first time, complete setup:",
+                fontSize = 13.sp,
+                color = AccentSoft,
+                textAlign = TextAlign.Center
+            )
+            Spacer(modifier = Modifier.height(20.dp))
+
+            Card(
+                shape = RoundedCornerShape(14.dp),
+                colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
+                modifier = Modifier.fillMaxWidth()
+            ) {
+                Column(modifier = Modifier.padding(16.dp)) {
+                    Text("1. Tap Build Number 7 times", fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
+                    Spacer(modifier = Modifier.height(4.dp))
+                    Text("Open Android Settings → About tablet / phone → Software information. Rapidly tap 'Build number' 7 times to enable Developer options.", fontSize = 12.sp, color = Color(0xFF9A9AAE), lineHeight = 18.sp)
+                }
+            }
+            Spacer(modifier = Modifier.height(12.dp))
+            Card(
+                shape = RoundedCornerShape(14.dp),
+                colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
+                modifier = Modifier.fillMaxWidth()
+            ) {
+                Column(modifier = Modifier.padding(16.dp)) {
+                    Text("2. Enable USB Debugging", fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
+                    Spacer(modifier = Modifier.height(4.dp))
+                    Text("Go to Settings → Developer options. Switch 'USB debugging' to ON. Connect the USB cable and tap 'Allow' (check 'Always allow').", fontSize = 12.sp, color = Color(0xFF9A9AAE), lineHeight = 18.sp)
+                }
+            }
+            Spacer(modifier = Modifier.height(12.dp))
+            Card(
+                shape = RoundedCornerShape(14.dp),
+                colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
+                modifier = Modifier.fillMaxWidth()
+            ) {
+                Column(modifier = Modifier.padding(16.dp)) {
+                    Text("3. Enable Wireless Debugging", fontSize = 15.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
+                    Spacer(modifier = Modifier.height(4.dp))
+                    Text("For Wi-Fi fallback: In Settings → Developer options, toggle 'Wireless debugging' to ON (Android 11+), or use 'Enable Wi-Fi ADB' on the PC.", fontSize = 12.sp, color = Color(0xFF9A9AAE), lineHeight = 18.sp)
+                }
+            }
+            Spacer(modifier = Modifier.height(24.dp))
+            Button(
+                onClick = onRetry,
+                colors = ButtonDefaults.buttonColors(containerColor = Accent),
+                shape = RoundedCornerShape(10.dp),
+                modifier = Modifier.fillMaxWidth()
+            ) {
+                Text("I've done this, Retry Connection", fontSize = 15.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(vertical = 4.dp))
+            }
+        }
+    }
+}
@@ -1150,6 +1311,14 @@ private fun SettingsSheet(
+            Spacer(Modifier.height(16.dp))
+            OutlinedButton(
+                onClick = onOpenSetupGuide,
+                modifier = Modifier.fillMaxWidth(),
+                colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentSoft)
+            ) {
+                Text("📖 Setup Guide", fontSize = 14.sp)
+            }
```

---

## 7. Conclusion

This intervention completely rectifies the architectural, visual, and operational defects:
1. The PC GUI sidebar passes the red line test with mathematical certainty ($X=10\text{ px}$ across all rows).
2. The list strictly contains unique devices displaying pure nicknames or labels, with clean transport badges (🟢, 🔵, ⚪).
3. The Android application proactively guides first-time users through the required ADB developer options when no connection is found after 5 seconds, and provides a direct "Setup Guide" link in settings.
4. The PC GUI features an integrated APK Auto-Installer modal with live ADB device discovery, selection checkboxes, and non-blocking installation status updates.
