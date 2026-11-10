/*
 * Overlays drawn above the stream: the 'unpaired by host' banner and the host notice overlay (paused, settings updating, app updating).
 * Owns what they look like; whether they show is decided by UScreenMain.
 */
package com.uscreen.ui

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import com.uscreen.HostNotice
import kotlinx.coroutines.delay

/**
 * Persistent cover shown after the host forgot this device. It stays until the host talks to the app
 * again, and offers to uninstall the app (the system asks the user to confirm).
 */
@Composable
fun UnpairedBanner() {
    val context = LocalContext.current
    /*
     * The system uninstall dialog can take a second to appear. From the tap until the dialog is answered
     * (uninstalled, or cancelled) the button shows a spinner and ignores further taps, so it never looks
     * dead and cannot launch the dialog twice.
     */
    var uninstalling by remember { mutableStateOf(false) }
    val uninstall = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        uninstalling = false
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .zIndex(300f)
            .background(Color(0xE6000000))
            .pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } } },
        contentAlignment = Alignment.Center
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.padding(32.dp)) {
            Text(
                text = "Unpaired by Host. This device is no longer connected.",
                fontSize = 22.sp,
                fontWeight = FontWeight.SemiBold,
                color = Color.White,
                textAlign = TextAlign.Center
            )
            Spacer(Modifier.height(20.dp))
            Button(
                enabled = !uninstalling,
                onClick = {
                    uninstalling = true
                    try {
                        uninstall.launch(
                            Intent(Intent.ACTION_UNINSTALL_PACKAGE, Uri.parse("package:${context.packageName}"))
                                .putExtra(Intent.EXTRA_RETURN_RESULT, true)
                        )
                    } catch (_: Exception) {
                        // No uninstaller to handle it: nothing will ever answer, so release the button now.
                        uninstalling = false
                    }
                }
            ) {
                if (uninstalling) {
                    CircularProgressIndicator(
                        modifier = Modifier.size(18.dp),
                        strokeWidth = 2.dp,
                        color = LocalContentColor.current
                    )
                    Spacer(Modifier.width(8.dp))
                    Text("Uninstalling…")
                } else {
                    Text("Uninstall App")
                }
            }
        }
    }
}

/**
 * Full-screen, semi-transparent cover shown while the host says the stream is paused, its settings
 * are being applied, or this app is being replaced. It swallows touches so nothing is sent to the
 * host meanwhile. A "settings updating" or "updating" overlay clears itself after 45 s in case the
 * host never says it is done.
 */
@Composable
fun HostNoticeOverlay(notice: HostNotice?) {
    var shown by remember { mutableStateOf(notice) }
    LaunchedEffect(notice) {
        shown = notice
        if (notice != null && notice.kind != HostNotice.Kind.PAUSED) {
            delay(45_000)
            shown = null
        }
    }
    AnimatedVisibility(
        visible = shown != null,
        enter = fadeIn(),
        exit = fadeOut(),
        modifier = Modifier.fillMaxSize().zIndex(200f)
    ) {
        val n = shown ?: notice
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color(0xB3000000))
                .pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent().changes.forEach { it.consume() } } },
            contentAlignment = Alignment.Center
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.padding(32.dp)) {
                if (n?.kind != HostNotice.Kind.PAUSED) {
                    CircularProgressIndicator(color = OnSurface, modifier = Modifier.size(36.dp))
                    Spacer(Modifier.height(16.dp))
                }
                Text(
                    text = when (n?.kind) {
                        HostNotice.Kind.PAUSED -> "Connection Paused by Host"
                        HostNotice.Kind.SETTINGS -> "Settings updating..."
                        HostNotice.Kind.UPDATING -> "Updating App... (Host: ${n?.host}, Device: ${n?.device})"
                        null -> ""
                    },
                    fontSize = 22.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = OnSurface,
                    textAlign = TextAlign.Center
                )
            }
        }
    }
}
