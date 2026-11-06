package com.uscreen.ui

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Build
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.ExperimentalAnimationApi
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowForward
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Monitor
import androidx.compose.material.icons.filled.PriorityHigh
import androidx.compose.material.icons.filled.Usb
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.Prefs
import com.uscreen.TouchCapture
import com.uscreen.VideoReceiver
import kotlinx.coroutines.delay

/**
 * Evaluates whether Developer Options are unlocked system-wide.
 */
fun isDevOptionsEnabled(context: Context): Boolean {
    return try {
        Settings.Global.getInt(
            context.contentResolver,
            Settings.Global.DEVELOPMENT_SETTINGS_ENABLED,
            0
        ) != 0
    } catch (_: Exception) {
        true
    }
}

/**
 * Evaluates whether USB Debugging (adb) is currently enabled.
 */
fun isUsbAdbEnabled(context: Context): Boolean {
    return try {
        Settings.Global.getInt(
            context.contentResolver,
            Settings.Global.ADB_ENABLED,
            0
        ) != 0
    } catch (_: Exception) {
        true
    }
}

/**
 * Evaluates whether Wireless Debugging (adb over TLS / Wi-Fi) is enabled.
 */
fun isWifiAdbEnabled(context: Context): Boolean {
    return try {
        Settings.Global.getInt(
            context.contentResolver,
            "adb_wifi_enabled",
            0
        ) != 0
    } catch (_: Exception) {
        false
    }
}

/**
 * Whether something on the UScreen host is actually answering on its video socket.
 *
 * Wireless Debugging being switched on says nothing about the host, so this opens the socket and
 * sends nothing: the host stays silent until it sees a token, so a read that times out means a
 * live listener, while a refused connect, a connect timeout or an immediate EOF (an adb reverse
 * with nothing behind it) mean nobody is there.
 */
suspend fun probeHostSocket(host: String, port: Int): Boolean = withContext(Dispatchers.IO) {
    try {
        Socket().use { s ->
            s.connect(InetSocketAddress(host, port), 1500)
            s.soTimeout = 800
            try {
                s.getInputStream().read() != -1
            } catch (_: SocketTimeoutException) {
                true
            }
        }
    } catch (_: Exception) {
        false
    }
}

/**
 * Checks whether physical USB connection to host is established.
 */
fun checkUsbConnected(context: Context): Boolean {
    val usbManager = context.getSystemService(Context.USB_SERVICE) as? UsbManager
    val hasUsb = try {
        (usbManager?.deviceList?.isNotEmpty() == true) || (usbManager?.accessoryList?.isNotEmpty() == true)
    } catch (_: Exception) {
        false
    }
    val filter = IntentFilter("android.hardware.usb.action.USB_STATE")
    val intent = context.registerReceiver(null, filter)
    val usbConnected = intent != null && intent.getBooleanExtra("connected", false)
    val batteryIntent = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
    val plugged = batteryIntent?.getIntExtra(BatteryManager.EXTRA_PLUGGED, -1) ?: -1
    val isPluggedUsb = plugged == BatteryManager.BATTERY_PLUGGED_USB || plugged == BatteryManager.BATTERY_PLUGGED_AC
    return hasUsb || usbConnected || isPluggedUsb
}

/**
 * Checks whether device is connected to local Wi-Fi.
 */
fun checkWifiConnected(context: Context): Boolean {
    val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return false
    val net = cm.activeNetwork ?: return false
    val caps = cm.getNetworkCapabilities(net) ?: return false
    return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)
}

/**
 * Discrete connectivity diagnostic health classifications.
 */
enum class DiagnosticState {
    NO_TRANSPORT,
    USB_UNAUTHORIZED,
    SOCKET_REFUSED,
    WIFI_HOST_UNREACHABLE,
    CONNECTING
}

/**
 * Synthesize raw transport state, socket diagnostics, and elapsed waiting time into actionable user telemetry.
 */
fun evaluateDiagnostic(
    usbPlugged: Boolean,
    wifiOn: Boolean,
    preferredTransport: Int,
    dynamicDiagnostic: String?,
    secondsWaiting: Int
): Pair<DiagnosticState, String> {
    val transportOk = when (preferredTransport) {
        Prefs.TRANSPORT_USB -> usbPlugged
        Prefs.TRANSPORT_WIFI -> wifiOn
        else -> usbPlugged || wifiOn
    }
    val state = when {
        !transportOk -> DiagnosticState.NO_TRANSPORT
        dynamicDiagnostic?.contains("refused", ignoreCase = true) == true -> DiagnosticState.SOCKET_REFUSED
        !usbPlugged && wifiOn && (secondsWaiting > 4 || dynamicDiagnostic?.contains("unreachable", ignoreCase = true) == true) -> DiagnosticState.WIFI_HOST_UNREACHABLE
        else -> DiagnosticState.CONNECTING
    }
    val message = when (state) {
        DiagnosticState.NO_TRANSPORT -> when (preferredTransport) {
            Prefs.TRANSPORT_USB -> "Please connect a USB cable."
            Prefs.TRANSPORT_WIFI -> "Please connect to a Wi-Fi network."
            else -> "Please connect a USB cable or ensure Wi-Fi is enabled."
        }
        DiagnosticState.USB_UNAUTHORIZED -> "Waiting for UScreen desktop..."
        DiagnosticState.SOCKET_REFUSED -> "Connection refused. Is UScreen running on your PC?"
        DiagnosticState.WIFI_HOST_UNREACHABLE -> "Wi-Fi connected, but host unreachable. Are you on the same network?"
        DiagnosticState.CONNECTING -> "Waiting for UScreen desktop..."
    }
    return Pair(state, message)
}

/**
 * Step navigation indicator pill row.
 */
@Composable
fun StepIndicator(currentStep: Int, totalSteps: Int = 4) {
    Row(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.padding(vertical = 12.dp)
    ) {
        for (i in 0 until totalSteps) {
            val isActive = i == currentStep
            val isDone = i < currentStep
            Box(
                modifier = Modifier
                    .size(if (isActive) 24.dp else 12.dp, 12.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(
                        when {
                            isActive -> AccentSoft
                            isDone -> Ok
                            else -> SurfaceHigh
                        }
                    )
            )
        }
    }
}

/**
 * Status checklist row used inside the wizard screens.
 */
@Composable
fun StatusItemCard(
    title: String,
    description: String,
    isPassed: Boolean,
    actionButtonText: String? = null,
    onActionClick: (() -> Unit)? = null
) {
    // Status rows are a dense list: standard (calm) motion, not the wizard's expressive springs.
    CalmMotion {
        StatusItemCardContent(title, description, isPassed, actionButtonText, onActionClick)
    }
}

@Composable
private fun StatusItemCardContent(
    title: String,
    description: String,
    isPassed: Boolean,
    actionButtonText: String? = null,
    onActionClick: (() -> Unit)? = null
) {
    Card(
        shape = RoundedCornerShape(8.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (isPassed) Color(0x3310B981) else MaterialTheme.colorScheme.surfaceContainer
        ),
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth()
            ) {
                Surface(
                    shape = RoundedCornerShape(4.dp),
                    color = if (isPassed) Ok else Subtle,
                    modifier = Modifier.size(28.dp)
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Icon(
                            imageVector = if (isPassed) Icons.Default.Check else Icons.Default.PriorityHigh,
                            contentDescription = null,
                            tint = OnSurface,
                            modifier = Modifier.size(16.dp)
                        )
                    }
                }
                Spacer(Modifier.width(12.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = title,
                        fontSize = 15.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = OnSurface
                    )
                    Text(
                        text = description,
                        fontSize = 12.sp,
                        color = if (isPassed) Color(0xFFCFE9DB) else Muted
                    )
                }
            }

            if (!isPassed && actionButtonText != null && onActionClick != null) {
                Spacer(Modifier.height(8.dp))
                MorphButton(
                    onClick = onActionClick,
                    colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.primary, contentColor = MaterialTheme.colorScheme.onPrimary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(start = 40.dp)
                ) {
                    Text(actionButtonText, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                }
            }
        }
    }
}

/**
 * 4-Screen Guided Onboarding Wizard & Connection Monitor.
 */
@OptIn(ExperimentalAnimationApi::class, ExperimentalMaterial3Api::class, androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun OnboardingScreen(
    videoReceiver: VideoReceiver?,
    @Suppress("UNUSED_PARAMETER") touchCapture: TouchCapture?,
    prefs: Prefs?,
    isConnected: Boolean,
    forceReviewMode: Boolean = false,
    onDismissReviewMode: () -> Unit = {}
) {
    val context = LocalContext.current
    var hasCompletedOnboarding by remember {
        mutableStateOf(prefs?.hasCompletedOnboarding ?: false)
    }
    var currentStep by remember { mutableIntStateOf(0) }

    // Live status polling
    var devOptionsOn by remember { mutableStateOf(isDevOptionsEnabled(context)) }
    var usbAdbOn by remember { mutableStateOf(isUsbAdbEnabled(context)) }
    var wifiAdbOn by remember { mutableStateOf(isWifiAdbEnabled(context)) }
    var usbPlugged by remember { mutableStateOf(checkUsbConnected(context)) }
    var wifiConnected by remember { mutableStateOf(checkWifiConnected(context)) }
    var hostReachable by remember { mutableStateOf(false) }
    val streamLive by rememberUpdatedState(isConnected)
    var showHelpWizard by remember { mutableStateOf(false) }
    var dynamicDiagnostic by remember { mutableStateOf<String?>(null) }
    var secondsWaiting by remember { mutableIntStateOf(0) }
    val preferredTransport = prefs?.preferredTransport ?: Prefs.TRANSPORT_AUTO

    // Reset step to 0 when entering review mode
    LaunchedEffect(forceReviewMode) {
        if (forceReviewMode) {
            currentStep = 0
        }
    }

    LaunchedEffect(videoReceiver) {
        videoReceiver?.onDiagnosticUpdate = { diag ->
            (context as? ComponentActivity)?.runOnUiThread {
                dynamicDiagnostic = diag
            }
        }
    }

    LaunchedEffect(Unit) {
        while (true) {
            devOptionsOn = isDevOptionsEnabled(context)
            usbAdbOn = isUsbAdbEnabled(context)
            wifiAdbOn = isWifiAdbEnabled(context)
            usbPlugged = checkUsbConnected(context)
            wifiConnected = checkWifiConnected(context)
            secondsWaiting += 1
            delay(1000)
        }
    }

    // Wireless is only "ready" once the host really answers, not merely because the toggle is on.
    LaunchedEffect(wifiAdbOn, videoReceiver) {
        while (true) {
            hostReachable = streamLive || (
                wifiAdbOn && probeHostSocket(videoReceiver?.host ?: VideoReceiver.HOST, videoReceiver?.port ?: VideoReceiver.PORT)
            )
            delay(3000)
        }
    }
    val wifiReady = wifiAdbOn && hostReachable

    val showWizard = !hasCompletedOnboarding || forceReviewMode

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Surface0),
        contentAlignment = Alignment.Center
    ) {
        if (showWizard) {
            // 4-Step Guided Onboarding Wizard
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(horizontal = 28.dp, vertical = 24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.SpaceBetween
            ) {
                // Header
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        text = "UScreen Pro",
                        fontSize = 32.sp,
                        fontWeight = FontWeight.Bold,
                        color = OnSurface
                    )
                    Text(
                        text = "Step ${currentStep + 1} of 4",
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        color = AccentSoft
                    )
                    StepIndicator(currentStep = currentStep, totalSteps = 4)
                }

                // Wizard Step Body with Smooth Horizontal Animation
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                    contentAlignment = Alignment.Center
                ) {
                    // Expressive scheme: bouncy spatial spring for the slide, effects spring for the fade.
                    val slideSpec = MaterialTheme.motionScheme.defaultSpatialSpec<IntOffset>()
                    val fadeSpec = MaterialTheme.motionScheme.defaultEffectsSpec<Float>()
                    AnimatedContent(
                        targetState = currentStep,
                        transitionSpec = {
                            if (targetState > initialState) {
                                (slideInHorizontally(slideSpec) { width -> width } + fadeIn(fadeSpec)).togetherWith(
                                    slideOutHorizontally(slideSpec) { width -> -width } + fadeOut(fadeSpec)
                                )
                            } else {
                                (slideInHorizontally(slideSpec) { width -> -width } + fadeIn(fadeSpec)).togetherWith(
                                    slideOutHorizontally(slideSpec) { width -> width } + fadeOut(fadeSpec)
                                )
                            }
                        },
                        label = "WizardStepTransition"
                    ) { step ->
                        when (step) {
                            // SCREEN 1: Welcome & Intro
                            0 -> {
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .verticalScroll(rememberScrollState())
                                        .padding(vertical = 12.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally
                                ) {
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = Color(0x334F46E5),
                                        modifier = Modifier.size(72.dp)
                                    ) {
                                        Box(contentAlignment = Alignment.Center) {
                                            Icon(
                                                imageVector = Icons.Default.Monitor,
                                                contentDescription = null,
                                                tint = AccentSoft,
                                                modifier = Modifier.size(36.dp)
                                            )
                                        }
                                    }
                                    Spacer(Modifier.height(16.dp))
                                    Text(
                                        text = "High-Performance Linux Display",
                                        fontSize = 22.sp,
                                        fontWeight = FontWeight.Bold,
                                        color = OnSurface,
                                        textAlign = TextAlign.Center
                                    )
                                    Spacer(Modifier.height(8.dp))
                                    Text(
                                        text = "Turn your Android tablet into a blazing-fast secondary monitor " +
                                                "and pressure-sensitive graphics tablet for Linux with zero lag.",
                                        fontSize = 14.sp,
                                        lineHeight = 22.sp,
                                        color = Muted,
                                        textAlign = TextAlign.Center,
                                        modifier = Modifier.padding(horizontal = 16.dp)
                                    )
                                    Spacer(Modifier.height(20.dp))

                                    Card(
                                        colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
                                        shape = RoundedCornerShape(8.dp),
                                        modifier = Modifier.fillMaxWidth(0.9f)
                                    ) {
                                        Column(modifier = Modifier.padding(16.dp)) {
                                            Text("Key Capabilities", fontSize = 14.sp, fontWeight = FontWeight.Bold, color = AccentSoft)
                                            Spacer(Modifier.height(8.dp))
                                            Text("• Hardware H.264 & HEVC ultra-low latency streaming", fontSize = 12.sp, color = OnSurface)
                                            Spacer(Modifier.height(4.dp))
                                            Text("• S-Pen / Stylus pressure & tilt input via uinput", fontSize = 12.sp, color = OnSurface)
                                            Spacer(Modifier.height(4.dp))
                                            Text("• Seamless USB cable & high-speed Wi-Fi pairing", fontSize = 12.sp, color = OnSurface)
                                        }
                                    }
                                }
                            }

                            // SCREEN 2: Enable USB Debugging
                            1 -> {
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .verticalScroll(rememberScrollState())
                                        .padding(vertical = 12.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally
                                ) {
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = if (devOptionsOn && usbAdbOn) Color(0x3310B981) else Color(0x33F59E0B),
                                        modifier = Modifier.size(64.dp)
                                    ) {
                                        Box(contentAlignment = Alignment.Center) {
                                            Icon(
                                                imageVector = if (devOptionsOn && usbAdbOn) Icons.Default.Check else Icons.Default.Usb,
                                                contentDescription = null,
                                                tint = if (devOptionsOn && usbAdbOn) Ok else Warn,
                                                modifier = Modifier.size(32.dp)
                                            )
                                        }
                                    }
                                    Spacer(Modifier.height(12.dp))
                                    Text(
                                        text = "Enable USB Debugging",
                                        fontSize = 22.sp,
                                        fontWeight = FontWeight.Bold,
                                        color = OnSurface,
                                        textAlign = TextAlign.Center
                                    )
                                    Spacer(Modifier.height(4.dp))
                                    Text(
                                        text = "UScreen establishes its secure virtual display tunnel via Android ADB.",
                                        fontSize = 13.sp,
                                        color = Muted,
                                        textAlign = TextAlign.Center
                                    )
                                    Spacer(Modifier.height(16.dp))

                                    StatusItemCard(
                                        title = "1. Developer Options",
                                        description = if (devOptionsOn) "Developer Options are unlocked." else "Tap 'Build number' 7 times in About Device.",
                                        isPassed = devOptionsOn,
                                        actionButtonText = if (!devOptionsOn) "Open About Device" else null,
                                        onActionClick = {
                                            try {
                                                context.startActivity(Intent(Settings.ACTION_DEVICE_INFO_SETTINGS))
                                            } catch (_: Exception) {
                                                context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                            }
                                        }
                                    )

                                    StatusItemCard(
                                        title = "2. USB Debugging",
                                        description = if (usbAdbOn) "USB Debugging is enabled." else "Turn on 'USB debugging' in Developer Options.",
                                        isPassed = usbAdbOn,
                                        actionButtonText = if (!usbAdbOn) "Open Developer Options" else null,
                                        onActionClick = {
                                            try {
                                                context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                            } catch (_: Exception) {
                                                context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                            }
                                        }
                                    )
                                }
                            }

                            // SCREEN 3: Enable Wireless Debugging
                            2 -> {
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .verticalScroll(rememberScrollState())
                                        .padding(vertical = 12.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally
                                ) {
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = if (wifiReady || (Build.VERSION.SDK_INT < Build.VERSION_CODES.R && usbAdbOn)) Color(0x3310B981) else Color(0x333B82F6),
                                        modifier = Modifier.size(64.dp)
                                    ) {
                                        Box(contentAlignment = Alignment.Center) {
                                            Icon(
                                                imageVector = if (wifiReady) Icons.Default.Check else Icons.Default.Wifi,
                                                contentDescription = null,
                                                tint = if (wifiReady) Ok else AccentSoft,
                                                modifier = Modifier.size(32.dp)
                                            )
                                        }
                                    }
                                    Spacer(Modifier.height(12.dp))
                                    Text(
                                        text = "Enable Wireless Debugging",
                                        fontSize = 22.sp,
                                        fontWeight = FontWeight.Bold,
                                        color = OnSurface,
                                        textAlign = TextAlign.Center
                                    )
                                    Spacer(Modifier.height(4.dp))
                                    Text(
                                        text = "For untethered wireless streaming, enable Wireless Debugging or scan the QR code on your PC.",
                                        fontSize = 13.sp,
                                        color = Muted,
                                        textAlign = TextAlign.Center
                                    )
                                    Spacer(Modifier.height(16.dp))

                                    StatusItemCard(
                                        title = "Wi-Fi Network",
                                        description = if (wifiConnected) "Tablet is connected to local Wi-Fi." else "Connect to the same Wi-Fi network as your PC.",
                                        isPassed = wifiConnected,
                                        actionButtonText = if (!wifiConnected) "Open Wi-Fi Settings" else null,
                                        onActionClick = {
                                            try {
                                                context.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))
                                            } catch (_: Exception) {
                                                context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                            }
                                        }
                                    )

                                    val isAndroid11Plus = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
                                    StatusItemCard(
                                        title = if (isAndroid11Plus) "Wireless Debugging (Android 11+)" else "USB Pairing Mode (Legacy)",
                                        description = when {
                                            wifiReady -> "Wireless Debugging: Ready"
                                            wifiAdbOn -> "Wireless Debugging: Enabled, but not connected to host."
                                            isAndroid11Plus -> "Turn on 'Wireless debugging' in Developer Options to pair wirelessly."
                                            else -> "Plug in USB once to automatically initialize wireless TCP mode."
                                        },
                                        isPassed = wifiReady || (!isAndroid11Plus && usbPlugged),
                                        actionButtonText = if (!wifiAdbOn && isAndroid11Plus) "Open Wireless Debugging" else null,
                                        onActionClick = {
                                            try {
                                                context.startActivity(Intent("android.settings.WIRELESS_DEBUGGING_SETTINGS"))
                                            } catch (_: Exception) {
                                                try {
                                                    context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                                } catch (_: Exception) {
                                                    context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                                }
                                            }
                                        }
                                    )
                                }
                            }

                            // SCREEN 4: Ready
                            3 -> {
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .verticalScroll(rememberScrollState())
                                        .padding(vertical = 12.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally
                                ) {
                                    Surface(
                                        shape = RoundedCornerShape(4.dp),
                                        color = Color(0x3310B981),
                                        modifier = Modifier.size(80.dp)
                                    ) {
                                        Box(contentAlignment = Alignment.Center) {
                                            Icon(
                                                imageVector = Icons.Default.Check,
                                                contentDescription = null,
                                                tint = Ok,
                                                modifier = Modifier.size(48.dp)
                                            )
                                        }
                                    }
                                    Spacer(Modifier.height(16.dp))
                                    Text(
                                        text = "You're All Set!",
                                        fontSize = 26.sp,
                                        fontWeight = FontWeight.Bold,
                                        color = OnSurface,
                                        textAlign = TextAlign.Center
                                    )
                                    Spacer(Modifier.height(8.dp))
                                    Text(
                                        text = "Your tablet is fully prepared. Connect via USB cable or launch " +
                                                "UScreen Pro on your Linux workstation to begin streaming.",
                                        fontSize = 14.sp,
                                        lineHeight = 22.sp,
                                        color = Muted,
                                        textAlign = TextAlign.Center,
                                        modifier = Modifier.padding(horizontal = 16.dp)
                                    )
                                    Spacer(Modifier.height(24.dp))

                                    // Quick checklist summary
                                    Card(
                                        shape = RoundedCornerShape(8.dp),
                                        colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
                                        modifier = Modifier.fillMaxWidth(0.9f)
                                    ) {
                                        Column(modifier = Modifier.padding(16.dp)) {
                                            Row(verticalAlignment = Alignment.CenterVertically) {
                                                Icon(
                                                    imageVector = if (usbAdbOn) Icons.Default.Check else Icons.Default.PriorityHigh,
                                                    contentDescription = null,
                                                    tint = if (usbAdbOn) Ok else Warn,
                                                    modifier = Modifier.size(16.dp)
                                                )
                                                Spacer(Modifier.width(8.dp))
                                                Text("USB Debugging: ${if (usbAdbOn) "Active" else "Pending"}", fontSize = 13.sp, color = OnSurface)
                                            }
                                            Spacer(Modifier.height(4.dp))
                                            Row(verticalAlignment = Alignment.CenterVertically) {
                                                Icon(
                                                    imageVector = if (wifiReady) Icons.Default.Check else Icons.Default.PriorityHigh,
                                                    contentDescription = null,
                                                    tint = if (wifiReady) Ok else Subtle,
                                                    modifier = Modifier.size(16.dp)
                                                )
                                                Spacer(Modifier.width(8.dp))
                                                Text(if (wifiReady) "Wireless Debugging: Ready" else if (wifiAdbOn) "Wireless Debugging: Enabled, but not connected to host" else "Wireless Debugging: Available", fontSize = 13.sp, color = OnSurface)
                                            }
                                            Spacer(Modifier.height(4.dp))
                                            Row(verticalAlignment = Alignment.CenterVertically) {
                                                Icon(
                                                    imageVector = Icons.Default.Check,
                                                    contentDescription = null,
                                                    tint = Ok,
                                                    modifier = Modifier.size(16.dp)
                                                )
                                                Spacer(Modifier.width(8.dp))
                                                Text("Transport: ${if (usbPlugged) "USB Cable Detected" else if (wifiConnected) "Wi-Fi Connected" else "None"}", fontSize = 13.sp, color = OnSurface)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Wizard Bottom Navigation Bar
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 16.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (currentStep > 0) {
                        OutlinedButton(
                            onClick = { currentStep -= 1 },
                            colors = ButtonDefaults.outlinedButtonColors(contentColor = Muted)
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(
                                    imageVector = Icons.Default.ArrowBack,
                                    contentDescription = null,
                                    tint = Muted,
                                    modifier = Modifier.size(16.dp).padding(end = 4.dp)
                                )
                                Text("Back", fontSize = 14.sp)
                            }
                        }
                    } else {
                        Spacer(Modifier.width(80.dp))
                    }

                    if (currentStep < 3) {
                        MorphButton(
                            onClick = { currentStep += 1 },
                            colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.primary, contentColor = MaterialTheme.colorScheme.onPrimary)
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text("Next", fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                                Icon(
                                    imageVector = Icons.Default.ArrowForward,
                                    contentDescription = null,
                                    tint = OnSurface,
                                    modifier = Modifier.size(16.dp).padding(start = 4.dp)
                                )
                            }
                        }
                    } else {
                        MorphButton(
                            onClick = {
                                prefs?.hasCompletedOnboarding = true
                                hasCompletedOnboarding = true
                                if (forceReviewMode) {
                                    onDismissReviewMode()
                                }
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = Ok),
                            modifier = Modifier.padding(horizontal = 8.dp)
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Icon(
                                    imageVector = Icons.Default.Check,
                                    contentDescription = null,
                                    tint = OnSurface,
                                    modifier = Modifier.size(16.dp).padding(end = 4.dp)
                                )
                                Text("Done", fontSize = 16.sp, fontWeight = FontWeight.Bold, color = OnSurface)
                            }
                        }
                    }
                }
            }
        } else {
            // Normal Waiting for Desktop Connection Screen
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                modifier = Modifier.padding(24.dp)
            ) {
                Text(
                    text = "UScreen",
                    fontSize = 42.sp,
                    fontWeight = FontWeight.Bold,
                    color = OnSurface
                )
                Text(
                    text = "USB & Wireless Display",
                    fontSize = 15.sp,
                    color = AccentSoft
                )
                Spacer(modifier = Modifier.height(32.dp))

                CircularProgressIndicator(
                    color = Accent,
                    strokeWidth = 4.dp,
                    modifier = Modifier.size(48.dp)
                )
                Spacer(modifier = Modifier.height(24.dp))

                val (diagState, diagMessage) = evaluateDiagnostic(
                    usbPlugged = usbPlugged,
                    wifiOn = wifiConnected,
                    preferredTransport = preferredTransport,
                    dynamicDiagnostic = dynamicDiagnostic,
                    secondsWaiting = secondsWaiting
                )

                Text(
                    text = diagMessage,
                    fontSize = 14.sp,
                    lineHeight = 20.sp,
                    color = if (diagState == DiagnosticState.CONNECTING) Muted else Warn,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = 32.dp)
                )
                Spacer(modifier = Modifier.height(28.dp))

                OutlinedButton(
                    onClick = { showHelpWizard = true },
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentSoft)
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Icon(
                            imageVector = Icons.Default.Build,
                            contentDescription = null,
                            tint = AccentSoft,
                            modifier = Modifier.size(16.dp).padding(end = 4.dp)
                        )
                        Text("Help Me Fix It", fontSize = 13.sp)
                    }
                }
            }

            // Troubleshooter Bottom Sheet
            if (showHelpWizard) {
                val transportOk = when (preferredTransport) {
                    Prefs.TRANSPORT_USB -> usbPlugged
                    Prefs.TRANSPORT_WIFI -> wifiConnected
                    else -> usbPlugged || wifiConnected
                }

                ModalBottomSheet(
                    onDismissRequest = { showHelpWizard = false },
                    containerColor = SurfaceLow
                ) {
                    AnimatedVisibility(
                        visible = true,
                        enter = fadeIn() + expandVertically(),
                        exit = fadeOut() + shrinkVertically()
                    ) {
                        Column(
                            modifier = Modifier
                                .verticalScroll(rememberScrollState())
                                .padding(horizontal = 24.dp, vertical = 12.dp)
                        ) {
                            Text(
                                "Connection Troubleshooter",
                                fontSize = 20.sp,
                                fontWeight = FontWeight.Bold,
                                color = OnSurface
                            )
                            Spacer(Modifier.height(8.dp))
                            Text(
                                "Let's verify each requirement step-by-step:",
                                fontSize = 13.sp,
                                color = Muted
                            )
                            Spacer(Modifier.height(16.dp))

                            StatusItemCard(
                                title = "1. Network Transport",
                                description = if (transportOk) "Physical or wireless transport active." else "Please connect USB cable or Wi-Fi.",
                                isPassed = transportOk,
                                actionButtonText = if (!transportOk) "Open Wi-Fi Settings" else null,
                                onActionClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                }
                            )

                            StatusItemCard(
                                title = "2. Developer Options",
                                description = if (devOptionsOn) "Developer options enabled." else "Disabled. Must tap Build number 7 times in About Device.",
                                isPassed = devOptionsOn,
                                actionButtonText = if (!devOptionsOn) "Open About Device" else null,
                                onActionClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_DEVICE_INFO_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                }
                            )

                            StatusItemCard(
                                title = "3. USB / Wireless Debugging",
                                description = if (usbAdbOn || wifiAdbOn) "Debugging is enabled." else "Disabled. Must toggle USB or Wireless debugging in Developer Options.",
                                isPassed = usbAdbOn || wifiAdbOn,
                                actionButtonText = if (!usbAdbOn && !wifiAdbOn) "Open Developer Options" else null,
                                onActionClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                }
                            )

                            Spacer(Modifier.height(20.dp))
                            Button(
                                onClick = {
                                    showHelpWizard = false
                                    currentStep = 0
                                    hasCompletedOnboarding = false
                                    prefs?.hasCompletedOnboarding = false
                                },
                                colors = ButtonDefaults.buttonColors(),
                                modifier = Modifier.fillMaxWidth()
                            ) {
                                Text("Restart Full Setup Wizard", fontSize = 13.sp)
                            }
                            Spacer(Modifier.height(16.dp))
                        }
                    }
                }
            }
        }
    }
}
