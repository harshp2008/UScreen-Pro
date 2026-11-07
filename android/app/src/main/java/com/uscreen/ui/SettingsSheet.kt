package com.uscreen.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.MenuBook
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.IconButton
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.filled.Close
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.BuildConfig
import com.uscreen.Prefs
import com.uscreen.getLocalWifiIp
import kotlin.math.roundToInt

/**
 * Full-screen settings page in a dialog window. It replaces the modal bottom sheet, whose drag-to-dismiss
 * gesture competed with scrolling the list; a dialog has no draggable surface, so a drag always scrolls.
 * Back and the close button dismiss it.
 */
@Composable
private fun SettingsContainer(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Surface(modifier = Modifier.fillMaxSize(), color = SurfaceLow) {
            Box(modifier = Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
                content()
                IconButton(onClick = onDismiss, modifier = Modifier.align(Alignment.TopEnd)) {
                    Icon(Icons.Default.Close, contentDescription = "Close settings", tint = OnSurface)
                }
            }
        }
    }
}

/**
 * Settings configuration page.
 *
 * Exposes live-tunable streaming parameters (video bitrate, target FPS), device mode toggling
 * (Display vs Graphics Tablet), display orientation locking, touch input filters, power management
 * (WakeLock and lock-screen display persistence), preferred transport selection, and wireless pairing setup guidance.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsSheet(
    prefs: Prefs?,
    penOnly: Boolean,
    onPenOnlyChange: (Boolean) -> Unit,
    showStats: Boolean,
    onShowStatsChange: (Boolean) -> Unit,
    blankPenOnly: Boolean,
    onBlankPenOnlyChange: (Boolean) -> Unit,
    touchEnabled: Boolean,
    onTouchEnabledChange: (Boolean) -> Unit,
    orientation: Int,
    onOrientationChange: (Int) -> Unit,
    onApply: (bitrateKbps: Int, fps: Int) -> Unit,
    immersiveModeEnabled: Boolean = false,
    onImmersiveModeEnabledChange: (Boolean) -> Unit = {},
    keepConnectedInBackground: Boolean = false,
    onKeepConnectedInBackgroundChange: (Boolean) -> Unit = {},
    keepDisplayAliveWhenLocked: Boolean = true,
    onKeepDisplayAliveWhenLockedChange: (Boolean) -> Unit = {},
    touchOnLockScreen: Boolean = false,
    touchOnLockScreenSupported: Boolean = true,
    onTouchOnLockScreenChange: (Boolean) -> Unit = {},
    preferredTransport: Int = Prefs.TRANSPORT_AUTO,
    onPreferredTransportChange: (Int) -> Unit = {},
    themeMode: Int = Prefs.THEME_SYSTEM,
    onThemeModeChange: (Int) -> Unit = {},
    onExitApp: () -> Unit = {},
    onDismiss: () -> Unit,
) {
    var bitrateMbps by remember {
        mutableStateOf((prefs?.bitrateKbps ?: Prefs.DEFAULT_BITRATE_KBPS) / 1000f)
    }
    var fpsChoice by remember { mutableStateOf(prefs?.fps ?: Prefs.DEFAULT_FPS) }
    val context = LocalContext.current

    SettingsContainer(onDismiss = onDismiss) {
      // Dense utility rows: calm (standard) motion and 40dp minimum touch targets, matching the desktop app.
      CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides 40.dp) {
      CalmMotion {
        // The scroll container is the whole page (full width, full height), so a drag that starts on blank
        // background scrolls too. The padding sits inside the scrolling area for the same reason.
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp)
        ) {
            Text(
                "Settings",
                fontSize = 20.sp,
                fontWeight = FontWeight.Bold,
                color = OnSurface
            )
            Spacer(Modifier.height(20.dp))

            // Mode toggle: Graphics Tablet vs Screen
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Graphics tablet", fontSize = 14.sp, color = Muted)
                    Text(
                        "Draw on the computer's own screen with the pen, " +
                            "instead of showing a second screen here",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = penOnly,
                    onCheckedChange = onPenOnlyChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            // Orientation toggle
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Rotate automatically", fontSize = 14.sp, color = Muted)
                    Text(
                        "Follow the tilt sensor between the two landscape directions",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = orientation == Prefs.ORIENTATION_AUTO,
                    onCheckedChange = { auto ->
                        onOrientationChange(
                            if (auto) Prefs.ORIENTATION_AUTO else Prefs.ORIENTATION_CAMERA_DOWN
                        )
                    },
                    colors = SwitchDefaults.colors()
                )
            }
            if (orientation != Prefs.ORIENTATION_AUTO) {
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(
                        Prefs.ORIENTATION_CAMERA_UP to "Camera up",
                        Prefs.ORIENTATION_CAMERA_DOWN to "Camera down",
                    ).forEach { (value, label) ->
                        FilterChip(
                            selected = orientation == value,
                            onClick = { onOrientationChange(value) },
                            label = { Text(label) },
                            colors = FilterChipDefaults.filterChipColors()
                        )
                    }
                }
                Text(
                    "Camera down is the usual way to hold it for drawing.",
                    fontSize = 11.sp,
                    color = Muted
                )
            }
            Spacer(Modifier.height(20.dp))

            // Theme: Light, Dark or System default (AppCompatDelegate.setDefaultNightMode)
            Text("Theme", fontSize = 14.sp, color = Muted)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(
                    Prefs.THEME_LIGHT to "Light",
                    Prefs.THEME_DARK to "Dark",
                    Prefs.THEME_SYSTEM to "System (Auto)",
                ).forEach { (value, label) ->
                    FilterChip(
                        selected = themeMode == value,
                        onClick = { onThemeModeChange(value) },
                        label = { Text(label) },
                        colors = FilterChipDefaults.filterChipColors()
                    )
                }
            }
            Spacer(Modifier.height(20.dp))

            // Preferred Transport selector
            Text("Preferred transport", fontSize = 14.sp, color = Muted)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(
                    Prefs.TRANSPORT_USB to "USB",
                    Prefs.TRANSPORT_AUTO to "Auto",
                    Prefs.TRANSPORT_WIFI to "Wi-Fi",
                ).forEach { (value, label) ->
                    FilterChip(
                        selected = preferredTransport == value,
                        onClick = { onPreferredTransportChange(value) },
                        label = { Text(label) },
                        colors = FilterChipDefaults.filterChipColors()
                    )
                }
            }
            Text(
                when (preferredTransport) {
                    Prefs.TRANSPORT_USB -> "Strictly connect using a physical USB cable."
                    Prefs.TRANSPORT_WIFI -> "Strictly connect over Wi-Fi network."
                    else -> "Automatically use USB when plugged in, falling back to Wi-Fi."
                },
                fontSize = 11.sp,
                color = Muted
            )
            Spacer(Modifier.height(20.dp))

            // Pairing Setup Guide Section
            var guideExpanded by remember { mutableStateOf(false) }
            val isAndroid11Plus = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
            val localIp = remember { getLocalWifiIp(context) }

            Surface(
                color = SurfaceMid,
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(8.dp))
                    .clickable { guideExpanded = !guideExpanded }
            ) {
                Column(modifier = Modifier.padding(12.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Icon(
                            imageVector = Icons.Default.Info,
                            contentDescription = null,
                            tint = AccentSoft,
                            modifier = Modifier.padding(end = 8.dp)
                        )
                        Text(
                            text = "Pairing Setup Guide",
                            fontSize = 15.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = OnSurface,
                            modifier = Modifier.weight(1f)
                        )
                        Icon(
                            imageVector = if (guideExpanded) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown,
                            contentDescription = if (guideExpanded) "Collapse" else "Expand",
                            tint = Subtle
                        )
                    }

                    AnimatedVisibility(
                        visible = guideExpanded,
                        enter = fadeIn() + expandVertically(),
                        exit = fadeOut() + shrinkVertically()
                    ) {
                        Column(modifier = Modifier.padding(top = 8.dp)) {
                            if (isAndroid11Plus) {
                                Text(
                                    text = "Secure Wi-Fi Pairing (Android 11+):",
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = AccentSoft
                                )
                                Spacer(Modifier.height(4.dp))
                                Text(
                                    text = "1. Go to Settings → Developer Options → Wireless Debugging.\n" +
                                           "2. Tap 'Pair device with pairing code'.\n" +
                                           "3. Enter the 6-digit code in the UScreen PC app.",
                                    fontSize = 12.sp,
                                    color = Muted,
                                    lineHeight = 18.sp
                                )
                            } else {
                                Text(
                                    text = "Legacy Wi-Fi Pairing (Android <11):",
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Warn
                                )
                                Spacer(Modifier.height(4.dp))
                                Text(
                                    text = "1. Go to Settings → Developer Options → USB Debugging.\n" +
                                           "2. Plug in the USB cable once.\n" +
                                           "3. The PC will auto-enable Wi-Fi ADB.",
                                    fontSize = 12.sp,
                                    color = Muted,
                                    lineHeight = 18.sp
                                )
                            }

                            Spacer(Modifier.height(8.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(
                                    onClick = {
                                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                        val text = localIp ?: "192.168.1.1"
                                        val clip = ClipData.newPlainText("UScreen IP", text)
                                        clipboard.setPrimaryClip(clip)
                                        Toast.makeText(context, "Copied IP: $text", Toast.LENGTH_SHORT).show()
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = SurfaceHigh),
                                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp)
                                ) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Icon(
                                            imageVector = Icons.Default.ContentCopy,
                                            contentDescription = null,
                                            tint = OnSurface,
                                            modifier = Modifier.padding(end = 4.dp)
                                        )
                                        Text("Copy IP & Port", fontSize = 12.sp, color = OnSurface)
                                    }
                                }

                                Button(
                                    onClick = {
                                        try {
                                            context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                        } catch (_: Exception) {
                                            Toast.makeText(context, "Developer options not enabled", Toast.LENGTH_SHORT).show()
                                        }
                                    },
                                    colors = ButtonDefaults.buttonColors(),
                                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp)
                                ) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Icon(
                                            imageVector = Icons.Default.Settings,
                                            contentDescription = null,
                                            tint = OnSurface,
                                            modifier = Modifier.padding(end = 4.dp)
                                        )
                                        Text("Open Developer Options", fontSize = 12.sp, color = OnSurface)
                                    }
                                }
                            }

                        }
                    }
                }
            }
            Spacer(Modifier.height(20.dp))

            if (!penOnly) {
                Text(
                    "Bitrate: ${bitrateMbps.roundToInt()} Mbps",
                    fontSize = 14.sp,
                    color = Muted
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
                    "20 Mbps is plenty for text and UI. Going higher does not look sharper once " +
                        "the USB link is saturated — it only adds delay.",
                    fontSize = 11.sp,
                    color = Muted
                )
                Spacer(Modifier.height(20.dp))

                Text("Frame rate", fontSize = 14.sp, color = Muted)
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(30, 60, 90).forEach { f ->
                        FilterChip(
                            selected = fpsChoice == f,
                            onClick = { fpsChoice = f },
                            label = { Text("$f fps") },
                            colors = FilterChipDefaults.filterChipColors()
                        )
                    }
                }
                Spacer(Modifier.height(20.dp))
            }

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Show stats overlay", fontSize = 14.sp, color = Muted)
                    Text(
                        "FPS and bandwidth in the corner",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Switch(
                    checked = showStats,
                    onCheckedChange = onShowStatsChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Black screen in graphics tablet mode", fontSize = 14.sp, color = Muted)
                    Text(
                        "Nothing is streamed there, so the screen goes black a few seconds after " +
                            "it starts instead of showing the same text for hours",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = blankPenOnly,
                    onCheckedChange = onBlankPenOnlyChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Touch input", fontSize = 14.sp, color = Muted)
                    Text(
                        "Off sends only the pen, which keeps a resting hand from doing anything",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = touchEnabled,
                    onCheckedChange = onTouchEnabledChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Enable Immersive Mode (Hide Settings Button)", fontSize = 14.sp, color = Muted)
                    Text(
                        "Hides the gear button and maps the Android Back button to open settings",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = immersiveModeEnabled,
                    onCheckedChange = onImmersiveModeEnabledChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Keep connected in background", fontSize = 14.sp, color = Muted)
                    Text(
                        "Maintain stream and touch connection when switched away. Consumes more battery.",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = keepConnectedInBackground,
                    onCheckedChange = onKeepConnectedInBackgroundChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Keep display alive when locked", fontSize = 14.sp, color = Muted)
                    Text(
                        "Hold system WakeLock and keep this screen showing while the device is locked.",
                        fontSize = 11.sp,
                        color = Muted
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = keepDisplayAliveWhenLocked,
                    onCheckedChange = onKeepDisplayAliveWhenLockedChange,
                    colors = SwitchDefaults.colors()
                )
            }
            Spacer(Modifier.height(20.dp))

            // Touch while locked. Greyed out (with the reason as a long-press tooltip and as visible text)
            // when the device cannot deliver touches to the app over the lock screen.
            val lockHint = "Your device does not support touch injection on the lock screen."
            TooltipBox(
                positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
                tooltip = { if (!touchOnLockScreenSupported) PlainTooltip { Text(lockHint) } },
                state = rememberTooltipState(),
                enableUserInput = !touchOnLockScreenSupported
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            "Touch on Lock Screen",
                            fontSize = 14.sp,
                            color = if (touchOnLockScreenSupported) Muted else Subtle
                        )
                        Text(
                            if (touchOnLockScreenSupported)
                                "Off: a locked device shows a cover and sends no touch. On: touch keeps working while locked."
                            else lockHint,
                            fontSize = 11.sp,
                            color = if (touchOnLockScreenSupported) Muted else Subtle
                        )
                    }
                    Spacer(Modifier.width(12.dp))
                    Switch(
                        checked = touchOnLockScreen && touchOnLockScreenSupported,
                        onCheckedChange = onTouchOnLockScreenChange,
                        enabled = touchOnLockScreenSupported,
                        colors = SwitchDefaults.colors()
                    )
                }
            }
            Spacer(Modifier.height(24.dp))

            if (!penOnly) {
                Button(
                    onClick = {
                        onApply((bitrateMbps * 1000).roundToInt(), fpsChoice)
                        onDismiss()
                    },
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors()
                ) {
                    Text("Apply", fontSize = 16.sp, modifier = Modifier.padding(vertical = 4.dp))
                }
                Spacer(Modifier.height(8.dp))
                Text(
                    "Applying restarts the stream for a moment.",
                    fontSize = 11.sp,
                    color = Muted,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth()
                )
            }

            if (immersiveModeEnabled) {
                Spacer(Modifier.height(16.dp))
                OutlinedButton(
                    onClick = onExitApp,
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = ErrorTone)
                ) {
                    Text("Exit UScreen", fontSize = 14.sp)
                }
            }
            Spacer(Modifier.height(20.dp))

            val buildVer = BuildConfig.VERSION_NAME
            val isDirtyVer = buildVer.contains("-dirty")
            val displayVer = if (isDirtyVer) "$buildVer (Uncommitted Changes)" else buildVer
            Text(
                text = "UScreen Build: $displayVer",
                fontSize = 11.sp,
                color = if (isDirtyVer) Warn else Subtle,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp)
            )
        }
      }
      }
    }
}
