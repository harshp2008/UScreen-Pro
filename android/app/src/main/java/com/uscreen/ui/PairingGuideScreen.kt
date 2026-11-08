package com.uscreen.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonColors
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.getLocalWifiIp

/**
 * Dark green with white text, in both themes. The theme's own primary is a light mint in dark mode, and
 * white text on a light green fails MD3's contrast minimum (4.5:1); #047857 with white is 5.5:1.
 */
private val GuideGreen = Color(0xFF047857)
private val OnGuideGreen = Color(0xFFFFFFFF)

/** Button colors for the pairing guide and the "Setup Wireless Pairing" entry: primary (dark green) with on-primary (white). */
@Composable
internal fun guideButtonColors(): ButtonColors =
    ButtonDefaults.buttonColors(containerColor = GuideGreen, contentColor = OnGuideGreen)

/**
 * The wireless pairing guide as a page of its own (it used to be an accordion inside the settings list).
 * Android 11+ pairs with a code under Wireless Debugging; older versions switch to Wi-Fi ADB after one USB connection.
 */
@Composable
fun PairingGuideScreen(onDismiss: () -> Unit) {
    val context = LocalContext.current
    val isAndroid11Plus = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
    val localIp = remember { getLocalWifiIp(context) }

    FullScreenPage(onDismiss = onDismiss) {
        CalmMotion {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 24.dp, vertical = 8.dp)
            ) {
                Text("Wireless pairing", fontSize = 20.sp, fontWeight = FontWeight.Bold, color = OnSurface)
                Spacer(Modifier.height(16.dp))
                Surface(color = SurfaceMid, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text(
                            if (isAndroid11Plus) "Secure Wi-Fi pairing (Android 11+)" else "Legacy Wi-Fi pairing (Android 10 and older)",
                            fontSize = 15.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = if (isAndroid11Plus) AccentSoft else Warn
                        )
                        Spacer(Modifier.height(8.dp))
                        Text(
                            if (isAndroid11Plus)
                                "1. Go to Settings → Developer Options → Wireless Debugging.\n" +
                                    "2. Tap 'Pair device with pairing code'.\n" +
                                    "3. Enter the 6-digit code in the UScreen app on the computer."
                            else
                                "1. Go to Settings → Developer Options → USB Debugging.\n" +
                                    "2. Plug in the USB cable once.\n" +
                                    "3. The computer enables Wi-Fi ADB by itself.",
                            fontSize = 13.sp,
                            color = Muted,
                            lineHeight = 20.sp
                        )
                        Spacer(Modifier.height(16.dp))
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(
                                onClick = {
                                    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                                    val text = localIp ?: "192.168.1.1"
                                    clipboard.setPrimaryClip(ClipData.newPlainText("UScreen IP", text))
                                    Toast.makeText(context, "Copied IP: $text", Toast.LENGTH_SHORT).show()
                                },
                                colors = guideButtonColors(),
                                contentPadding = PaddingValues(horizontal = 14.dp, vertical = 6.dp)
                            ) {
                                Icon(Icons.Default.ContentCopy, contentDescription = null, modifier = Modifier.size(16.dp).padding(end = 0.dp))
                                Spacer(Modifier.size(6.dp))
                                Text("Copy IP", fontSize = 13.sp)
                            }
                            Button(
                                onClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                    } catch (_: Exception) {
                                        Toast.makeText(context, "Developer options not enabled", Toast.LENGTH_SHORT).show()
                                    }
                                },
                                colors = guideButtonColors(),
                                contentPadding = PaddingValues(horizontal = 14.dp, vertical = 6.dp)
                            ) {
                                Icon(Icons.Default.Settings, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(Modifier.size(6.dp))
                                Text("Open Developer Options", fontSize = 13.sp)
                            }
                        }
                    }
                }
            }
        }
    }
}
