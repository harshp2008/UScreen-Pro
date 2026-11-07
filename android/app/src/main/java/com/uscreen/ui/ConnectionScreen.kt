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
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.zIndex
import com.uscreen.BuildConfig
import com.uscreen.HostNotice
import com.uscreen.MainActivity
import com.uscreen.Prefs
import com.uscreen.TouchCapture
import com.uscreen.VideoReceiver
import kotlinx.coroutines.delay

/**
 * Screen displayed when running in Graphics Tablet mode.
 * Shows informational guidance and blanks out the OLED screen after a timeout to conserve battery.
 */
@Composable
fun PenOnlyScreen(blankAfterSeconds: Int) {
    var blank by remember { mutableStateOf(false) }
    LaunchedEffect(blankAfterSeconds) {
        blank = false
        if (blankAfterSeconds > 0) {
            delay(blankAfterSeconds * 1000L)
            blank = true
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = Alignment.Center
    ) {
        if (!blank) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                modifier = Modifier.padding(32.dp)
            ) {
                Text(
                    "Graphics tablet mode",
                    fontSize = 20.sp,
                    color = OnSurface,
                    fontWeight = FontWeight.Bold
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    "The pen drives your computer's screen.\n" +
                        "Touch input is sent back; no video is streamed here.",
                    fontSize = 14.sp,
                    color = Muted,
                    textAlign = TextAlign.Center
                )
                if (blankAfterSeconds > 0) {
                    Spacer(Modifier.height(16.dp))
                    Text(
                        "The screen goes black in a few seconds to spare OLED panels. " +
                            "Settings are behind the gear icon in the corner.",
                        fontSize = 12.sp,
                        color = Muted,
                        textAlign = TextAlign.Center
                    )
                }
            }
        }
    }
}

/**
 * Persistent cover shown after the host forgot this device. It stays until the host talks to the app
 * again, and offers to uninstall the app (the system asks the user to confirm).
 */
@Composable
fun UnpairedBanner() {
    val context = LocalContext.current
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
                color = OnSurface,
                textAlign = TextAlign.Center
            )
            Spacer(Modifier.height(20.dp))
            Button(onClick = {
                context.startActivity(
                    Intent(Intent.ACTION_DELETE, Uri.parse("package:${context.packageName}"))
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                )
            }) {
                Text("Uninstall App")
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
    onOrientationChange: (Int) -> Unit = {},
    onStreamingChange: (Boolean) -> Unit = {},
    prefs: Prefs? = null,
    isDeviceLocked: Boolean = false,
    hostVersion: String? = null,
    hostNotice: HostNotice? = null,
    unpairedByHost: Boolean = false,
) {
    var isConnected by remember { mutableStateOf(false) }
    LaunchedEffect(isConnected) { onStreamingChange(isConnected) }
    var fps by remember { mutableStateOf(0f) }
    var mbps by remember { mutableStateOf(0f) }
    var showSettings by remember { mutableStateOf(false) }
    var showStats by remember { mutableStateOf(prefs?.showStats ?: false) }
    var touchOn by remember { mutableStateOf(prefs?.touchEnabled ?: true) }
    var blankInPenMode by remember { mutableStateOf(prefs?.blankPenOnly ?: true) }
    var orientationChoice by remember { mutableStateOf(prefs?.orientation ?: Prefs.ORIENTATION_AUTO) }
    var immersiveOn by remember { mutableStateOf(prefs?.immersiveModeEnabled ?: false) }
    var keepConnectedInBackground by remember { mutableStateOf(prefs?.keepConnectedInBackground ?: false) }
    var keepDisplayAliveWhenLocked by remember { mutableStateOf(prefs?.keepDisplayAliveWhenLocked ?: true) }
    var preferredTransport by remember { mutableStateOf(prefs?.preferredTransport ?: Prefs.TRANSPORT_AUTO) }
    var forceReviewMode by remember { mutableStateOf(false) }
    var themeMode by remember { mutableStateOf(prefs?.themeMode ?: Prefs.THEME_SYSTEM) }

    /*
     * A live stream means setup is done: never show onboarding over it, and remember that so the wizard
     * does not come back on a later dropout (the connection monitor still does). The grace period keeps a
     * quick (re)connect, such as resuming the app, from flashing the screen. The wizard stays reachable on
     * demand through Settings > Review Setup Guide (forceReviewMode).
     */
    var onboardingGraceOver by remember { mutableStateOf(false) }
    LaunchedEffect(isConnected) {
        if (isConnected) {
            onboardingGraceOver = false
            prefs?.hasCompletedOnboarding = true
        } else {
            delay(1500)
            onboardingGraceOver = true
        }
    }

    val context = LocalContext.current
    val activity = context as? MainActivity

    BackHandler(enabled = immersiveOn && (isConnected || penOnly) && !showSettings) {
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
                        object : SurfaceHolder.Callback {
                            override fun surfaceCreated(holder: SurfaceHolder) {
                                onSurfaceReady(this@apply)
                            }
                            override fun surfaceChanged(
                                holder: SurfaceHolder,
                                format: Int, width: Int, height: Int
                            ) {
                                onSurfaceReady(this@apply)
                            }
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
            visible = (!isConnected && !penOnly && onboardingGraceOver) || forceReviewMode,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier
                .fillMaxSize()
                .zIndex(if (forceReviewMode) 100f else 0f)
        ) {
            OnboardingScreen(
                videoReceiver = videoReceiver,
                touchCapture = touchCapture,
                prefs = prefs,
                isConnected = isConnected,
                forceReviewMode = forceReviewMode,
                onDismissReviewMode = { forceReviewMode = false }
            )
        }

        if (isConnected && showStats) {
            Surface(
                color = Color(0x99000000),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier
                    .align(Alignment.TopStart)
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

        if (isDeviceLocked) {
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

        val showSettingsHandle = !immersiveOn || !isConnected

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
                penOnly = penOnly,
                onPenOnlyChange = { wantPenOnly ->
                    touchCapture?.sendMode(wantPenOnly)
                    showSettings = false
                },
                showStats = showStats,
                onShowStatsChange = {
                    showStats = it
                    prefs?.showStats = it
                },
                blankPenOnly = blankInPenMode,
                onBlankPenOnlyChange = {
                    blankInPenMode = it
                    prefs?.blankPenOnly = it
                },
                touchEnabled = touchOn,
                onTouchEnabledChange = {
                    touchOn = it
                    prefs?.touchEnabled = it
                    touchCapture?.setTouchEnabled(it)
                },
                orientation = orientationChoice,
                onOrientationChange = {
                    orientationChoice = it
                    onOrientationChange(it)
                },
                onApply = { bitrateKbps, newFps ->
                    prefs?.bitrateKbps = bitrateKbps
                    prefs?.fps = newFps
                    touchCapture?.sendConfig(bitrateKbps, newFps)
                },
                immersiveModeEnabled = immersiveOn,
                onImmersiveModeEnabledChange = {
                    immersiveOn = it
                    prefs?.immersiveModeEnabled = it
                    activity?.applyImmersiveMode(it)
                },
                keepConnectedInBackground = keepConnectedInBackground,
                onKeepConnectedInBackgroundChange = {
                    keepConnectedInBackground = it
                    prefs?.keepConnectedInBackground = it
                },
                keepDisplayAliveWhenLocked = keepDisplayAliveWhenLocked,
                onKeepDisplayAliveWhenLockedChange = {
                    keepDisplayAliveWhenLocked = it
                    prefs?.keepDisplayAliveWhenLocked = it
                    activity?.applyLockScreenFlags()
                },
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
                themeMode = themeMode,
                onThemeModeChange = {
                    themeMode = it
                    prefs?.themeMode = it
                    activity?.applyThemeMode(it)
                },
                onReviewSetupGuide = {
                    forceReviewMode = true
                },
                onExitApp = {
                    activity?.finish()
                },
                onDismiss = { showSettings = false }
            )
        }
    }
}
