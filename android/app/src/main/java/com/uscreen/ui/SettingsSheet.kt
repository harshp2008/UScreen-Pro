/*
 * The settings page: sections and rows for input mode, pen style, touchpad, power and display
 * options, shown full screen in a dialog.
 */
package com.uscreen.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.BatteryChargingFull
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material.icons.filled.TouchApp
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.Haptics
import com.uscreen.BuildConfig
import com.uscreen.ChargingStatus
import com.uscreen.HostConfig
import com.uscreen.Prefs
import com.uscreen.ScreenLock
import com.uscreen.TouchpadShape
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

/**
 * Settings page: four collapsible sections (Display & Input, Connection, Power & Battery, Advanced).
 *
 * Every switch and choice here is a setting the computer also has on the device's page in UScreen Pro; a
 * change made here goes to the computer ([onSetting]) and one made there arrives as a `config_update`, so
 * [hostConfig] is the single source of what is shown. Only the bitrate and frame rate go through Apply,
 * because changing them restarts the stream.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsSheet(
    prefs: Prefs?,
    hostConfig: HostConfig,
    onSetting: (key: String, value: Any) -> Unit,
    onApply: (bitrateKbps: Int, fps: Int) -> Unit,
    touchOnLockScreenSupported: Boolean = true,
    preferredTransport: Int = Prefs.TRANSPORT_AUTO,
    onPreferredTransportChange: (Int) -> Unit = {},
    onOpenPairingGuide: () -> Unit = {},
    onExitApp: () -> Unit = {},
    onDismiss: () -> Unit,
) {
    var bitrateMbps by remember {
        mutableStateOf((prefs?.bitrateKbps ?: Prefs.DEFAULT_BITRATE_KBPS) / 1000f)
    }
    var fpsChoice by remember { mutableStateOf(prefs?.fps ?: Prefs.DEFAULT_FPS) }
    val context = LocalContext.current
    // Charging status, refreshed while the page is open (the current reading changes second to second).
    var chargingText by remember { mutableStateOf(ChargingStatus.read(context)) }
    LaunchedEffect(Unit) {
        while (true) {
            chargingText = ChargingStatus.read(context)
            delay(2000)
        }
    }
    // Whether the Device Admin role (needed to lock the screen) is granted; re-read after the system dialog closes.
    var canLock by remember { mutableStateOf(ScreenLock.canLock(context)) }
    LaunchedEffect(hostConfig.batterySaverLock) {
        while (true) {
            canLock = ScreenLock.canLock(context)
            delay(1000)
        }
    }
    val mode = hostConfig.inputMode

    FullScreenPage(onDismiss = onDismiss) {
        // Dense utility rows: calm (standard) motion and 40dp minimum touch targets, matching the desktop app.
        CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 40.dp) {
            CalmMotion {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .verticalScroll(rememberScrollState())
                        .padding(horizontal = 24.dp, vertical = 8.dp)
                ) {
                    Text("Settings", fontSize = 20.sp, fontWeight = FontWeight.Bold, color = OnSurface)
                    Spacer(Modifier.height(16.dp))

                    // ---- Display & Input -------------------------------------------------------------
                    SettingsSection("Display & Input", Icons.Default.TouchApp, startExpanded = true) {
                        Text("Input mode", fontSize = 14.sp, color = OnSurface, modifier = Modifier.padding(top = 12.dp))
                        Spacer(Modifier.height(8.dp))
                        val modes = listOf(
                            HostConfig.MODE_DISPLAY to "Extended Display",
                            HostConfig.MODE_TOUCHPAD to "Touchpad",
                            HostConfig.MODE_STYLUS to "Stylus",
                        )
                        SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
                            modes.forEachIndexed { index, (value, label) ->
                                SegmentedButton(
                                    selected = mode == value,
                                    onClick = { onSetting("input_mode", value) },
                                    shape = SegmentedButtonDefaults.itemShape(index, modes.size),
                                ) { Text(label, fontSize = 12.sp, maxLines = 1) }
                            }
                        }
                        Text(
                            when (mode) {
                                HostConfig.MODE_TOUCHPAD -> "A laptop touchpad. No video, so it saves battery."
                                HostConfig.MODE_STYLUS -> "The pen draws on this computer's own screen. No video."
                                else -> "A second screen for this computer."
                            },
                            fontSize = 11.sp,
                            color = Muted,
                            modifier = Modifier.padding(top = 8.dp)
                        )

                        SwitchRow(
                            "Touch input",
                            "Off sends only the pen, so a resting hand does nothing. Touchpad mode always uses fingers.",
                            hostConfig.touchInput || mode == HostConfig.MODE_TOUCHPAD,
                            { onSetting("touch_input", it) },
                            enabled = mode != HostConfig.MODE_TOUCHPAD
                        )
                        if (mode == HostConfig.MODE_STYLUS) {
                            ChipChoice(
                                "Pen acts as",
                                listOf(
                                    HostConfig.STYLUS_AUTO to "Dynamic (experimental)",
                                    HostConfig.STYLUS_MOUSE to "Mouse",
                                    HostConfig.STYLUS_TABLET to "Graphics tablet",
                                ),
                                hostConfig.stylusStyle,
                            ) { onSetting("stylus_style", it) }
                            if (hostConfig.stylusStyle == HostConfig.STYLUS_AUTO) {
                                Text(
                                    "Experimental: tested only on GNOME (Ubuntu). On other desktops choose Mouse or Graphics tablet.",
                                    fontSize = 11.sp,
                                    color = Muted
                                )
                            }
                            if (hostConfig.stylusStyle == HostConfig.STYLUS_MOUSE || hostConfig.stylusStyle == HostConfig.STYLUS_AUTO) {
                                val penButtons = listOf(
                                    HostConfig.PEN_BUTTON_RIGHT to "Right click",
                                    HostConfig.PEN_BUTTON_MIDDLE to "Middle click",
                                    HostConfig.PEN_BUTTON_NONE to "Nothing",
                                )
                                ChipChoice("Pen button", penButtons, hostConfig.stylusButtonPrimary) {
                                    onSetting("stylus_button_primary", it)
                                }
                                ChipChoice("Second pen button", penButtons, hostConfig.stylusButtonSecondary) {
                                    onSetting("stylus_button_secondary", it)
                                }
                                Text(
                                    "The tip is the left button and stays down while you drag, so you can select text " +
                                        "and move windows. Hovering moves the cursor.",
                                    fontSize = 11.sp,
                                    color = Muted
                                )
                            }
                            SwitchRow(
                                "Pen menu",
                                "Hold the pen still on the computer's screen for a small menu: cut, copy, paste, select all, undo.",
                                hostConfig.penMenu,
                                { onSetting("pen_menu", it) }
                            )
                            if (hostConfig.penMenu) {
                                ChipChoice(
                                    "Hold time for the menu",
                                    listOf("500" to "0.5 s", "800" to "0.8 s", "1000" to "1 s", "1500" to "1.5 s"),
                                    hostConfig.penMenuHoldMs.toString(),
                                ) { onSetting("pen_menu_hold_ms", it.toInt()) }
                                ChipChoice(
                                    "Menu buttons",
                                    listOf("text" to "Text", "icons" to "Icons"),
                                    hostConfig.penMenuStyle,
                                ) { onSetting("pen_menu_style", it) }
                            }
                            SwitchRow(
                                "Black screen in stylus mode",
                                "Nothing is streamed, so the screen goes black a few seconds after it starts.",
                                hostConfig.blankPenOnly,
                                { onSetting("blank_pen_only", it) }
                            )
                        }
                        SwitchRow(
                            "Immersive mode",
                            "Hides the gear button and maps the Back button to open settings.",
                            hostConfig.immersiveMode,
                            { onSetting("immersive_mode", it) }
                        )
                        // The counter shows video fps and bandwidth, so only Display mode has one.
                        if (mode == HostConfig.MODE_DISPLAY) {
                            SwitchRow(
                                "Show stats overlay",
                                "FPS and bandwidth in the corner.",
                                hostConfig.showStats,
                                { onSetting("show_stats", it) }
                            )
                        }
                        if (mode == HostConfig.MODE_TOUCHPAD) {
                            ChipChoice(
                                "Touchpad style",
                                listOf("clickpad" to "Clickpad", "classic" to "Classic buttons"),
                                hostConfig.touchpadStyle,
                            ) { onSetting("touchpad_style", it) }
                            if (hostConfig.touchpadStyle == HostConfig.STYLE_CLASSIC) {
                                SwitchRow(
                                    "Middle button",
                                    "Add a middle button between the left and right ones.",
                                    hostConfig.touchpadMiddleButton,
                                    { onSetting("touchpad_middle_button", it) }
                                )
                                SwitchRow(
                                    "Tap to click",
                                    "Let a tap on the pad click. Off, like old pads: use the buttons.",
                                    hostConfig.touchpadClassicTap,
                                    { onSetting("touchpad_classic_tap", it) }
                                )
                            }
                            val hasMotor = remember { Haptics.hasMotor(context) }
                            SwitchRow(
                                "Touchpad Haptics",
                                if (hasMotor) "Vibrate on clicks." else "No vibration motor detected on this device, so nothing will vibrate.",
                                hostConfig.touchpadHaptics,
                                { onSetting("touchpad_haptics", it) }
                            )
                            if (hostConfig.touchpadHaptics && hasMotor) {
                                ChipChoice(
                                    "Haptic strength",
                                    listOf("light" to "Light", "medium" to "Medium", "strong" to "Strong"),
                                    hostConfig.touchpadHapticStrength,
                                ) { onSetting("touchpad_haptic_strength", it) }
                            }
                            SwitchRow(
                                "Click sound",
                                "A short tick from the speaker on clicks.",
                                hostConfig.touchpadClickSound,
                                { onSetting("touchpad_click_sound", it) }
                            )
                            SwitchRow(
                                "Use Native Gestures (Wayland/libinput)",
                                "Your fingers are passed to the computer as a real touchpad, so swipes, pinch and " +
                                    "workspace gestures follow your hand. Turns itself off on X11 and desktops " +
                                    "without libinput gestures.",
                                hostConfig.nativeGestures,
                                { onSetting("native_gestures", it) }
                            )
                            if (hostConfig.nativeGestures) {
                                var sens by remember(hostConfig.touchpadSensitivity) {
                                    mutableFloatStateOf(hostConfig.touchpadSensitivity.toFloat())
                                }
                                Text(
                                    "Sensitivity: ${sens.roundToInt()}%",
                                    fontSize = 14.sp,
                                    color = OnSurface,
                                    modifier = Modifier.padding(top = 8.dp)
                                )
                                Slider(
                                    value = sens,
                                    onValueChange = { sens = it },
                                    onValueChangeFinished = { onSetting("touchpad_sensitivity", sens.roundToInt()) },
                                    valueRange = 50f..200f,
                                    colors = SliderDefaults.colors()
                                )
                                Text(
                                    "How far the pointer and swipes travel for each centimetre of finger movement. " +
                                        "Pointer acceleration on top of it is your desktop's touchpad speed setting.",
                                    fontSize = 11.sp,
                                    color = Muted
                                )
                            }
                            if (hostConfig.touchpadStyle == HostConfig.STYLE_CLICKPAD) ChipChoice(
                                "Right Click Method",
                                listOf(
                                    HostConfig.RIGHT_CLICK_ZONE to "Bottom Right Corner",
                                    HostConfig.RIGHT_CLICK_TWO_FINGER to "Two-Finger Tap",
                                ),
                                hostConfig.rightClickMethod,
                            ) { onSetting("right_click_method", it) }
                            ChipChoice(
                                "Touchpad Shape",
                                listOf(
                                    TouchpadShape.FULL to "Full Screen",
                                    TouchpadShape.STANDARD to "Standard Rectangle",
                                    TouchpadShape.WIDE to "Wide Rectangle",
                                ),
                                hostConfig.touchpadShape,
                            ) { onSetting("touchpad_shape", it) }
                            Text(
                                "Only the shaded rectangle responds; touches outside it are ignored. Standard is a centred 4:3, " +
                                    "Wide a centred 16:9.",
                                fontSize = 11.sp,
                                color = Muted
                            )
                            Text(
                                "Bottom Right Zone: tap the right half of the strip along the bottom edge of the touch area. " +
                                    "With native gestures the computer decides the rest: tap-to-click, two-finger tap, " +
                                    "and 3 / 4 finger swipes use its own touchpad settings. Without them, two-finger tap is " +
                                    "also a right click, three-finger tap is middle click, and swiping three or four " +
                                    "fingers sideways switches workspace (up or down: overview).",
                                fontSize = 11.sp,
                                color = Muted
                            )
                        }

                        ChipChoice(
                            "Orientation",
                            listOf("auto" to "Auto", "camera_up" to "Camera up", "camera_down" to "Camera down"),
                            hostConfig.orientation,
                        ) { onSetting("orientation", it) }
                        Text(
                            "Camera down is the usual way to hold it for drawing.",
                            fontSize = 11.sp,
                            color = Muted
                        )
                        ChipChoice(
                            "Theme",
                            listOf("light" to "Light", "dark" to "Dark", "system" to "System (Auto)"),
                            hostConfig.themeMode,
                        ) { onSetting("theme_mode", it) }

                        if (mode == HostConfig.MODE_DISPLAY) {
                            Text(
                                "Bitrate: ${bitrateMbps.roundToInt()} Mbps",
                                fontSize = 14.sp,
                                color = OnSurface,
                                modifier = Modifier.padding(top = 16.dp)
                            )
                            Slider(
                                value = bitrateMbps,
                                onValueChange = { bitrateMbps = it },
                                valueRange = (Prefs.MIN_BITRATE_KBPS / 1000).toFloat()..
                                    (Prefs.MAX_BITRATE_KBPS / 1000).toFloat(),
                                steps = 10,
                                colors = SliderDefaults.colors()
                            )
                            Text(
                                "20 Mbps is plenty for text and UI. Higher does not look sharper once the link is saturated.",
                                fontSize = 11.sp,
                                color = Muted
                            )
                            ChipChoice("Frame rate", listOf(30, 60, 90).map { it to "$it fps" }, fpsChoice) { fpsChoice = it }
                            Spacer(Modifier.height(12.dp))
                            Button(
                                onClick = {
                                    onApply((bitrateMbps * 1000).roundToInt(), fpsChoice)
                                    onDismiss()
                                },
                                modifier = Modifier.fillMaxWidth(),
                                colors = ButtonDefaults.buttonColors()
                            ) { Text("Apply", fontSize = 15.sp, modifier = Modifier.padding(vertical = 2.dp)) }
                            Text(
                                "Applying restarts the stream for a moment.",
                                fontSize = 11.sp,
                                color = Muted,
                                textAlign = TextAlign.Center,
                                modifier = Modifier.fillMaxWidth().padding(top = 4.dp)
                            )
                        }
                    }
                    Spacer(Modifier.height(12.dp))

                    // ---- Connection ------------------------------------------------------------------
                    SettingsSection("Connection", Icons.Default.Wifi) {
                        ChipChoice(
                            "Preferred transport",
                            listOf(
                                Prefs.TRANSPORT_USB to "USB",
                                Prefs.TRANSPORT_AUTO to "Auto",
                                Prefs.TRANSPORT_WIFI to "Wi-Fi",
                            ),
                            preferredTransport,
                            onPreferredTransportChange
                        )
                        Text(
                            when (preferredTransport) {
                                Prefs.TRANSPORT_USB -> "Strictly connect using a physical USB cable."
                                Prefs.TRANSPORT_WIFI -> "Strictly connect over the Wi-Fi network."
                                else -> "Use USB when plugged in, falling back to Wi-Fi."
                            },
                            fontSize = 11.sp,
                            color = Muted
                        )
                        // Only useful while the computer has no Wi-Fi address for this tablet.
                        if (!hostConfig.wifiConfigured) {
                            Spacer(Modifier.height(16.dp))
                            Button(
                                onClick = onOpenPairingGuide,
                                modifier = Modifier.fillMaxWidth(),
                                colors = guideButtonColors()
                            ) { Text("Setup Wireless Pairing", fontSize = 14.sp) }
                        }
                    }
                    Spacer(Modifier.height(12.dp))

                    // ---- Power & Battery -------------------------------------------------------------
                    SettingsSection("Power & Battery", Icons.Default.BatteryChargingFull) {
                        SwitchRow(
                            "Battery saver on disconnect",
                            "If the computer is unreachable for 2 minutes: stop retrying and dim to 10%. Tap Retry to reconnect.",
                            hostConfig.batterySaverOnDisconnect,
                            { onSetting("battery_saver_on_disconnect", it) }
                        )
                        SwitchRow(
                            "Require Device Admin Lock",
                            "Off: battery saver only dims and stops retrying. On: it also locks the tablet, which needs the " +
                                "Device Admin permission.",
                            hostConfig.batterySaverLock,
                            { onSetting("battery_saver_lock", it) },
                            enabled = hostConfig.batterySaverOnDisconnect
                        )
                        if (hostConfig.batterySaverOnDisconnect && hostConfig.batterySaverLock && !canLock) {
                            Spacer(Modifier.height(8.dp))
                            OutlinedButton(
                                onClick = {
                                    try { context.startActivity(ScreenLock.requestIntent(context)) } catch (_: Exception) {}
                                },
                                modifier = Modifier.fillMaxWidth()
                            ) { Text("Grant Device Admin", fontSize = 13.sp) }
                        }
                        if (canLock) {
                            Spacer(Modifier.height(8.dp))
                            Text(
                                "Device Admin already added. Remove it before uninstalling UScreen, since Android " +
                                    "blocks uninstalling an active device admin.",
                                fontSize = 12.sp,
                                color = Muted
                            )
                            Spacer(Modifier.height(6.dp))
                            OutlinedButton(
                                onClick = {
                                    ScreenLock.removeAdmin(context)
                                    canLock = ScreenLock.canLock(context)
                                },
                                modifier = Modifier.fillMaxWidth()
                            ) { Text("Remove Device Admin", fontSize = 13.sp) }
                        }
                        if (hostConfig.inputMode == HostConfig.MODE_DISPLAY) SwitchRow(
                            "Idle dimming",
                            "Dim the tablet when the pointer is away and it is unused. Extended Display mode only.",
                            hostConfig.idleDimEnabled,
                            { onSetting("idle_dim_enabled", it) }
                        )
                        if (hostConfig.inputMode == HostConfig.MODE_DISPLAY && hostConfig.idleDimEnabled) {
                            Text(
                                "After ${hostConfig.idleDimTimeoutSecs} s · dim ${"%.1f".format(hostConfig.dimDurationMs / 1000f)} s " +
                                    "(${curveText(hostConfig.dimCurve)}) · undim ${"%.1f".format(hostConfig.undimDurationMs / 1000f)} s " +
                                    "(${curveText(hostConfig.undimCurve)}). Curves and times are set in UScreen Pro on the computer.",
                                fontSize = 11.sp,
                                color = Muted,
                                modifier = Modifier.padding(top = 6.dp)
                            )
                        }
                        SwitchRow(
                            "Keep connected in background",
                            "Maintain stream and touch connection when switched away. Uses more battery.",
                            hostConfig.keepConnectedInBackground,
                            { onSetting("keep_connected_in_background", it) }
                        )
                    }
                    Spacer(Modifier.height(12.dp))

                    // ---- Advanced --------------------------------------------------------------------
                    SettingsSection("Advanced", Icons.Default.Tune) {
                        SwitchRow(
                            "Keep display alive when locked",
                            "Hold a wake lock and keep this screen showing while the device is locked.",
                            hostConfig.keepDisplayAliveWhenLocked,
                            { onSetting("keep_display_alive_when_locked", it) }
                        )
                        // Greyed out, with the reason as a long-press tooltip, where the device cannot deliver
                        // touches to the app over the lock screen.
                        val lockHint = "Your device does not support touch injection on the lock screen."
                        TooltipBox(
                            positionProvider = TooltipDefaults.rememberTooltipPositionProvider(),
                            tooltip = { if (!touchOnLockScreenSupported) PlainTooltip { Text(lockHint) } },
                            state = rememberTooltipState(),
                            enableUserInput = !touchOnLockScreenSupported
                        ) {
                            SwitchRow(
                                "Touch on lock screen",
                                if (touchOnLockScreenSupported)
                                    "Off: a locked device shows a cover and sends no touch. On: touch keeps working while locked."
                                else lockHint,
                                hostConfig.touchOnLockScreen && touchOnLockScreenSupported,
                                { onSetting("touch_on_lock_screen", it) },
                                enabled = touchOnLockScreenSupported
                            )
                        }
                        Text(
                            "Auto-launch, the session security token and encoder options are set in UScreen Pro on the computer.",
                            fontSize = 11.sp,
                            color = Muted,
                            modifier = Modifier.padding(top = 12.dp)
                        )
                        if (hostConfig.immersiveMode) {
                            Spacer(Modifier.height(12.dp))
                            OutlinedButton(
                                onClick = onExitApp,
                                modifier = Modifier.fillMaxWidth(),
                                colors = ButtonDefaults.outlinedButtonColors(contentColor = ErrorTone)
                            ) { Text("Exit UScreen", fontSize = 14.sp) }
                        }
                        Spacer(Modifier.height(12.dp))
                        Text(
                            text = chargingText,
                            fontSize = 12.sp,
                            color = Muted,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp)
                        )
                        val buildVer = BuildConfig.VERSION_NAME
                        val isDirtyVer = buildVer.contains("-dirty")
                        Text(
                            text = "UScreen Build: " + if (isDirtyVer) "$buildVer (Uncommitted Changes)" else buildVer,
                            fontSize = 11.sp,
                            color = if (isDirtyVer) Warn else Subtle,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth()
                        )
                    }
                    Spacer(Modifier.height(24.dp))
                }
            }
        }
    }
}
