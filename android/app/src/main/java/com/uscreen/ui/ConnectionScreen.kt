/*
 * The main screens shown while connected or waiting: graphics-tablet screen, touchpad screen,
 * unpaired banner, host notices, and the top-level `UScreenMain` composable.
 */
package com.uscreen.ui

import android.content.Intent
import android.net.Uri
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.zIndex
import com.uscreen.BuildConfig
import com.uscreen.HostConfig
import com.uscreen.HostNotice
import com.uscreen.MainActivity
import com.uscreen.Prefs
import com.uscreen.lockScreenTouchSupported
import com.uscreen.TouchCapture
import com.uscreen.VideoReceiver
import kotlinx.coroutines.delay
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue

/**
 * Root Composable orchestrating video rendering, touch capture, onboarding,
 * telemetry overlays, and settings sheet interactions.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun UScreenMain(
    onSurfaceReady: (SurfaceView) -> Unit,
    penOnly: Boolean = false,
    showThanks: Boolean = false,
    onDismissThanks: () -> Unit = {},
    onSurfaceDestroyed: () -> Unit = {},
    videoReceiver: VideoReceiver? = null,
    touchCapture: TouchCapture? = null,
    onStreamingChange: (Boolean) -> Unit = {},
    prefs: Prefs? = null,
    isDeviceLocked: Boolean = false,
    hostVersion: String? = null,
    hostNotice: HostNotice? = null,
    unpairedByHost: Boolean = false,
    /** The per-device settings the host pushed (touchpad, immersive, battery saver). */
    hostConfig: HostConfig = HostConfig(),
    /** The host has been unreachable for the whole 120 s reconnect window. */
    hostOffline: Boolean = false,
    /** Battery saver stopped the retry loops; only [onRetry] reconnects. */
    autoRetryDisabled: Boolean = false,
    onRetry: () -> Unit = {},
    /** A synced setting was changed on this tablet (the value is a Boolean, Int or String, as the key requires). */
    onSetting: (key: String, value: Any) -> Unit = { _, _ -> },
    /** The screen is faded down by idle dimming; a touch should only wake it. */
    idleDimmed: Boolean = false,
    onWake: () -> Unit = {},
) {
    var isConnected by remember { mutableStateOf(false) }
    LaunchedEffect(isConnected) { onStreamingChange(isConnected) }
    var fps by remember { mutableStateOf(0f) }
    var mbps by remember { mutableStateOf(0f) }
    var showSettings by remember { mutableStateOf(false) }
    val showStats = hostConfig.showStats
    val blankInPenMode = hostConfig.blankPenOnly
    val touchOnLockScreen = hostConfig.touchOnLockScreen
    val touchpadOn = hostConfig.inputMode == HostConfig.MODE_TOUCHPAD
    var showPairingGuide by remember { mutableStateOf(false) }
    val immersiveOn = hostConfig.immersiveMode
    val lockTouchSupported = remember { lockScreenTouchSupported() }
    /* Locked, and the user has not allowed touch there: cover the picture and send no input at all. */
    val lockBlocked = isDeviceLocked && !(touchOnLockScreen && lockTouchSupported)
    LaunchedEffect(lockBlocked, touchCapture) { touchCapture?.setInputBlocked(lockBlocked) }
    var preferredTransport by remember { mutableStateOf(prefs?.preferredTransport ?: Prefs.TRANSPORT_AUTO) }

    /*
     * The "waiting for host" screen shows whenever no stream is live. The grace period keeps a quick
     * (re)connect, such as resuming the app, from flashing it.
     */
    var onboardingGraceOver by remember { mutableStateOf(false) }
    LaunchedEffect(isConnected) {
        if (isConnected) {
            onboardingGraceOver = false
        } else {
            delay(1500)
            onboardingGraceOver = true
        }
    }

    val context = LocalContext.current
    val activity = context as? MainActivity

    BackHandler(enabled = immersiveOn && (isConnected || penOnly || touchpadOn) && !showSettings) {
        showSettings = true
    }

    LaunchedEffect(showSettings) {
        if (!showSettings) {
            activity?.applyImmersiveMode(immersiveOn)
        }
    }

    LaunchedEffect(videoReceiver) {
        videoReceiver?.onConnected = {
            (context as? ComponentActivity)?.runOnUiThread { isConnected = true }
        }
        videoReceiver?.onDisconnected = {
            (context as? ComponentActivity)?.runOnUiThread { isConnected = false }
        }
    }

    LaunchedEffect(isConnected, showStats) {
        while (isConnected) {
            delay(1000)
            fps = videoReceiver?.getFps() ?: 0f
            mbps = videoReceiver?.getMbps() ?: 0f
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
    ) {
        AndroidView(
            factory = { ctx ->
                SurfaceView(ctx).apply {
                    holder.setFormat(android.graphics.PixelFormat.OPAQUE)
                    holder.addCallback(
                        /**
                         * Reports the video surface to the caller when it appears or changes and when it goes away, so the decoder is attached to a live surface only.
                         */
                        object : SurfaceHolder.Callback {
                            /** The surface exists: attach the decoder. */
                            override fun surfaceCreated(holder: SurfaceHolder) {
                                onSurfaceReady(this@apply)
                            }
                            /** The surface size or format changed: attach the decoder again. */
                            override fun surfaceChanged(
                                holder: SurfaceHolder,
                                format: Int, width: Int, height: Int
                            ) {
                                onSurfaceReady(this@apply)
                            }
                            /** The surface is going away: detach the decoder. */
                            override fun surfaceDestroyed(holder: SurfaceHolder) {
                                onSurfaceDestroyed()
                            }
                        }
                    )
                }
            },
            modifier = Modifier.fillMaxSize()
        )

        AnimatedVisibility(
            visible = penOnly,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.fillMaxSize()
        ) {
            PenOnlyScreen(blankAfterSeconds = if (blankInPenMode) 8 else 0)
        }

        AnimatedVisibility(
            visible = touchpadOn && !penOnly,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.fillMaxSize()
        ) {
            TouchpadActiveScreen(
                showRightClickZone = hostConfig.rightClickMethod == com.uscreen.HostConfig.RIGHT_CLICK_ZONE,
                shape = hostConfig.touchpadShape,
            )
        }

        AnimatedVisibility(
            visible = !isConnected && !penOnly && !touchpadOn && onboardingGraceOver,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)
        ) {
            WaitingScreen(
                videoReceiver = videoReceiver,
                touchCapture = touchCapture,
                prefs = prefs,
                isConnected = isConnected,
                hostOffline = hostOffline,
                autoRetryDisabled = autoRetryDisabled,
                onRetry = onRetry,
                onOpenSettings = { showSettings = true },
            )
        }

        // The fps / Mbps counter belongs to the video stream, so it exists in Display mode only.
        if (isConnected && showStats && hostConfig.inputMode == com.uscreen.HostConfig.MODE_DISPLAY) {
            Surface(
                color = Color(0x99000000),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .windowInsetsPadding(WindowInsets.safeDrawing)
                    .padding(12.dp)
            ) {
                Text(
                    text = "%.0f fps   %.1f Mbps".format(fps, mbps),
                    fontSize = 12.sp,
                    color = Muted,
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp)
                )
            }
        }

        val isMismatch = hostVersion != null && hostVersion != BuildConfig.GIT_VERSION_STRING
        AnimatedVisibility(
            visible = isMismatch,
            enter = fadeIn() + expandVertically(),
            exit = fadeOut() + shrinkVertically(),
            modifier = Modifier
                .align(Alignment.TopCenter)
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .zIndex(50f)
        ) {
            Surface(
                color = Color(0xFFD32F2F),
                modifier = Modifier.fillMaxWidth()
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Default.Warning,
                        contentDescription = null,
                        tint = Warn,
                        modifier = Modifier.size(16.dp).padding(end = 4.dp)
                    )
                    Text(
                        text = "Host version mismatch (Host: ${hostVersion ?: "unknown"}, Device: ${BuildConfig.GIT_VERSION_STRING}). Please update on PC.",
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Bold,
                        color = OnSurface,
                        modifier = Modifier.weight(1f)
                    )
                }
            }
        }

        if (lockBlocked) {
            // Swallows every touch on the picture; the settings handle is drawn after this, so it still works.
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color(0x66000000))
                    .pointerInput(Unit) {
                        awaitPointerEventScope {
                            while (true) awaitPointerEvent().changes.forEach { it.consume() }
                        }
                    }
            )
            Surface(
                color = Color(0xD9B71C1C),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = if (isMismatch) 48.dp else 16.dp)
            ) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp)
                ) {
                    Icon(
                        imageVector = Icons.Default.Lock,
                        contentDescription = null,
                        tint = OnSurface,
                        modifier = Modifier.size(12.dp)
                    )
                    Spacer(Modifier.size(4.dp))
                    Text(
                        text = "Device Locked — Touch Disabled",
                        fontSize = 13.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = OnSurface
                    )
                }
            }
        }

        HostNoticeOverlay(hostNotice)
        if (unpairedByHost) UnpairedBanner()

        val showSettingsHandle = !immersiveOn || !(isConnected || touchpadOn)

        if (showSettingsHandle) {
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(8.dp)
                    .size(36.dp)
                    .alpha(if (penOnly && blankInPenMode) 0.18f else if (isConnected) 0.35f else 0.9f)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(0xAA141D1B))
                    .clickable { showSettings = true },
                contentAlignment = Alignment.Center
            ) {
                Icon(
                    imageVector = Icons.Default.Settings,
                    contentDescription = "Settings",
                    tint = OnSurface,
                    modifier = Modifier.size(16.dp)
                )
            }
        }

        if (showThanks) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(24.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(0xE6141D1B))
                    .padding(16.dp)
            ) {
                Column {
                    Text("UScreen is working.", fontSize = 15.sp, color = OnSurface, fontWeight = FontWeight.Bold)
                    Text(
                        "If it replaced a second monitor for you, a star on GitHub or a compatibility " +
                            "report helps other Linux users find it. This note appears only once.",
                        fontSize = 12.sp, color = Muted
                    )
                    Row(modifier = Modifier.padding(top = 8.dp)) {
                        Text(
                            "Open GitHub",
                            fontSize = 13.sp,
                            color = AccentSoft,
                            modifier = Modifier
                                .clickable {
                                    context.startActivity(
                                        Intent(
                                            Intent.ACTION_VIEW,
                                            Uri.parse("https://github.com/majmichu1/UScreen")
                                        )
                                    )
                                    onDismissThanks()
                                }
                                .padding(end = 20.dp)
                        )
                        Text(
                            "Dismiss",
                            fontSize = 13.sp,
                            color = Muted,
                            modifier = Modifier.clickable { onDismissThanks() }
                        )
                    }
                }
            }
        }

        if (showSettings) {
            SettingsSheet(
                prefs = prefs,
                hostConfig = hostConfig,
                onSetting = onSetting,
                onApply = { bitrateKbps, newFps ->
                    prefs?.bitrateKbps = bitrateKbps
                    prefs?.fps = newFps
                    touchCapture?.sendConfig(bitrateKbps, newFps)
                },
                touchOnLockScreenSupported = lockTouchSupported,
                preferredTransport = preferredTransport,
                onPreferredTransportChange = {
                    preferredTransport = it
                    prefs?.preferredTransport = it
                    val transStr = when (it) {
                        Prefs.TRANSPORT_WIFI -> "wifi"
                        Prefs.TRANSPORT_USB -> "usb"
                        else -> "auto"
                    }
                    touchCapture?.sendPreferredTransport(transStr)
                },
                onOpenPairingGuide = { showPairingGuide = true },
                onExitApp = { activity?.finish() },
                onDismiss = { showSettings = false }
            )
        }
        if (showPairingGuide) PairingGuideScreen(onDismiss = { showPairingGuide = false })

        // Wake on touch: while the screen is dimmed for idleness, the first touch anywhere only brings the
        // brightness back. It is consumed here so it never reaches the computer as a stray click.
        if (idleDimmed) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .zIndex(500f)
                    .pointerInput(Unit) {
                        awaitPointerEventScope {
                            while (true) {
                                val event = awaitPointerEvent()
                                if (event.type == PointerEventType.Press) onWake()
                                event.changes.forEach { it.consume() }
                            }
                        }
                    }
            )
        }
    }
}
