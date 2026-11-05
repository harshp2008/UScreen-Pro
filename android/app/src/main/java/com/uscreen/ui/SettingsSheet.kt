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
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.ModalBottomSheet
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
 * Settings configuration modal bottom sheet.
 *
 * Exposes live-tunable streaming parameters (video bitrate, target FPS), device mode toggling
 * (Display vs Graphics Tablet), display orientation locking, touch input filters, power management
 * (WakeLock and lock-screen display persistence), preferred transport selection, and wireless pairing setup guidance.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsSheet(
    prefs: Prefs?,
    updateAvailable: String?,
    onOpenUpdate: () -> Unit,
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
    preferredTransport: Int = Prefs.TRANSPORT_AUTO,
    onPreferredTransportChange: (Int) -> Unit = {},
    onExitApp: () -> Unit = {},
    onDismiss: () -> Unit,
) {
    var bitrateMbps by remember {
        mutableStateOf((prefs?.bitrateKbps ?: Prefs.DEFAULT_BITRATE_KBPS) / 1000f)
    }
    var fpsChoice by remember { mutableStateOf(prefs?.fps ?: Prefs.DEFAULT_FPS) }
    val context = LocalContext.current

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = Color(0xFF16161F)
    ) {
        Column(
            modifier = Modifier
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 8.dp)
        ) {
            Text(
                "Settings",
                fontSize = 20.sp,
                fontWeight = FontWeight.Bold,
                color = Color.White
            )
            Spacer(Modifier.height(20.dp))

            if (updateAvailable != null) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .background(Color(0x3350507A))
                        .clickable { onOpenUpdate() }
                        .padding(12.dp)
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Update available: $updateAvailable", fontSize = 14.sp, color = Color.White)
                        Text(
                            "Tap to open the release page. Update the desktop side too — they ship together.",
                            fontSize = 11.sp,
                            color = Color(0xFF9A9AB0)
                        )
                    }
                }
                Spacer(Modifier.height(16.dp))
            }

            // Mode toggle: Graphics Tablet vs Screen
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Graphics tablet", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "Draw on the computer's own screen with the pen, " +
                            "instead of showing a second screen here",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = penOnly,
                    onCheckedChange = onPenOnlyChange,
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
                )
            }
            Spacer(Modifier.height(20.dp))

            // Orientation toggle
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Rotate automatically", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "Follow the tilt sensor between the two landscape directions",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
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
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
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
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = Accent,
                                selectedLabelColor = Color.White
                            )
                        )
                    }
                }
                Text(
                    "Camera down is the usual way to hold it for drawing.",
                    fontSize = 11.sp,
                    color = Color(0xFF6A6A7E)
                )
            }
            Spacer(Modifier.height(20.dp))

            // Preferred Transport selector
            Text("Preferred transport", fontSize = 14.sp, color = Color(0xFFB0B0C0))
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(
                    Prefs.TRANSPORT_AUTO to "Auto",
                    Prefs.TRANSPORT_USB to "USB",
                    Prefs.TRANSPORT_WIFI to "Wi-Fi",
                ).forEach { (value, label) ->
                    FilterChip(
                        selected = preferredTransport == value,
                        onClick = { onPreferredTransportChange(value) },
                        label = { Text(label) },
                        colors = FilterChipDefaults.filterChipColors(
                            selectedContainerColor = Accent,
                            selectedLabelColor = Color.White
                        )
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
                color = Color(0xFF6A6A7E)
            )
            Spacer(Modifier.height(20.dp))

            // Pairing Setup Guide Section
            var guideExpanded by remember { mutableStateOf(false) }
            val isAndroid11Plus = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
            val localIp = remember { getLocalWifiIp(context) }

            Surface(
                color = Color(0xFF1E1E2C),
                shape = RoundedCornerShape(10.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .clickable { guideExpanded = !guideExpanded }
            ) {
                Column(modifier = Modifier.padding(14.dp)) {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.fillMaxWidth()
                    ) {
                        Text(
                            text = "🔗 Pairing Setup Guide",
                            fontSize = 15.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = Color.White,
                            modifier = Modifier.weight(1f)
                        )
                        Text(
                            text = if (guideExpanded) "▲" else "▼",
                            fontSize = 13.sp,
                            color = Color(0xFF9A9AB0)
                        )
                    }

                    AnimatedVisibility(
                        visible = guideExpanded,
                        enter = fadeIn() + expandVertically(),
                        exit = fadeOut() + shrinkVertically()
                    ) {
                        Column(modifier = Modifier.padding(top = 10.dp)) {
                            if (isAndroid11Plus) {
                                Text(
                                    text = "Secure Wi-Fi Pairing (Android 11+):",
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Color(0xFF64B5F6)
                                )
                                Spacer(Modifier.height(4.dp))
                                Text(
                                    text = "1. Go to Settings → Developer Options → Wireless Debugging.\n" +
                                           "2. Tap 'Pair device with pairing code'.\n" +
                                           "3. Enter the 6-digit code in the UScreen PC app.",
                                    fontSize = 12.sp,
                                    color = Color(0xFFD0D0E0),
                                    lineHeight = 18.sp
                                )
                            } else {
                                Text(
                                    text = "Legacy Wi-Fi Pairing (Android <11):",
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Bold,
                                    color = Color(0xFFFFB74D)
                                )
                                Spacer(Modifier.height(4.dp))
                                Text(
                                    text = "1. Go to Settings → Developer Options → USB Debugging.\n" +
                                           "2. Plug in the USB cable once.\n" +
                                           "3. The PC will auto-enable Wi-Fi ADB.",
                                    fontSize = 12.sp,
                                    color = Color(0xFFD0D0E0),
                                    lineHeight = 18.sp
                                )
                            }

                            Spacer(Modifier.height(10.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(
                                    onClick = {
                                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                        val text = localIp ?: "192.168.1.1"
                                        val clip = ClipData.newPlainText("UScreen IP", text)
                                        clipboard.setPrimaryClip(clip)
                                        Toast.makeText(context, "Copied IP: $text", Toast.LENGTH_SHORT).show()
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF2C2C3E)),
                                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
                                ) {
                                    Text("📋 Copy IP & Port", fontSize = 12.sp, color = Color.White)
                                }

                                Button(
                                    onClick = {
                                        try {
                                            context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                        } catch (_: Exception) {
                                            Toast.makeText(context, "Developer options not enabled", Toast.LENGTH_SHORT).show()
                                        }
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = Accent),
                                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp)
                                ) {
                                    Text("⚙ Open Developer Options", fontSize = 12.sp, color = Color.White)
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
                    color = Color(0xFFB0B0C0)
                )
                Slider(
                    value = bitrateMbps,
                    onValueChange = { bitrateMbps = it },
                    valueRange = (Prefs.MIN_BITRATE_KBPS / 1000).toFloat()..
                            (Prefs.MAX_BITRATE_KBPS / 1000).toFloat(),
                    steps = 10,
                    colors = SliderDefaults.colors(thumbColor = Accent, activeTrackColor = Accent)
                )
                Text(
                    "20 Mbps is plenty for text and UI. Going higher does not look sharper once " +
                        "the USB link is saturated — it only adds delay.",
                    fontSize = 11.sp,
                    color = Color(0xFF6A6A7E)
                )
                Spacer(Modifier.height(20.dp))

                Text("Frame rate", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(30, 60, 90).forEach { f ->
                        FilterChip(
                            selected = fpsChoice == f,
                            onClick = { fpsChoice = f },
                            label = { Text("$f fps") },
                            colors = FilterChipDefaults.filterChipColors(
                                selectedContainerColor = Accent,
                                selectedLabelColor = Color.White
                            )
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
                    Text("Show stats overlay", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "FPS and bandwidth in the corner",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
                    )
                }
                Switch(
                    checked = showStats,
                    onCheckedChange = onShowStatsChange,
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Black screen in graphics tablet mode", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "Nothing is streamed there, so the screen goes black a few seconds after " +
                            "it starts instead of showing the same text for hours",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = blankPenOnly,
                    onCheckedChange = onBlankPenOnlyChange,
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Touch input", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "Off sends only the pen, which keeps a resting hand from doing anything",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = touchEnabled,
                    onCheckedChange = onTouchEnabledChange,
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
                )
            }
            Spacer(Modifier.height(20.dp))

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
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Keep connected in background", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "Maintain stream and touch connection when switched away. Consumes more battery.",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = keepConnectedInBackground,
                    onCheckedChange = onKeepConnectedInBackgroundChange,
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
                )
            }
            Spacer(Modifier.height(20.dp))

            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Keep display alive when locked", fontSize = 14.sp, color = Color(0xFFB0B0C0))
                    Text(
                        "Hold system WakeLock while locked. Note: secure keyguard disables touch input until unlocked.",
                        fontSize = 11.sp,
                        color = Color(0xFF6A6A7E)
                    )
                }
                Spacer(Modifier.width(12.dp))
                Switch(
                    checked = keepDisplayAliveWhenLocked,
                    onCheckedChange = onKeepDisplayAliveWhenLockedChange,
                    colors = SwitchDefaults.colors(checkedTrackColor = Accent)
                )
            }
            Spacer(Modifier.height(24.dp))

            if (!penOnly) {
                Button(
                    onClick = {
                        onApply((bitrateMbps * 1000).roundToInt(), fpsChoice)
                        onDismiss()
                    },
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(containerColor = Accent)
                ) {
                    Text("Apply", fontSize = 16.sp, modifier = Modifier.padding(vertical = 4.dp))
                }
                Spacer(Modifier.height(8.dp))
                Text(
                    "Applying restarts the stream for a moment.",
                    fontSize = 11.sp,
                    color = Color(0xFF6A6A7E),
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth()
                )
            }

            if (immersiveModeEnabled) {
                Spacer(Modifier.height(16.dp))
                OutlinedButton(
                    onClick = onExitApp,
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color(0xFFFF6B6B))
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
                color = if (isDirtyVer) Color(0xFFFFB74D) else Color(0xFF6A6A7E),
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth().padding(bottom = 12.dp)
            )
        }
    }
}
