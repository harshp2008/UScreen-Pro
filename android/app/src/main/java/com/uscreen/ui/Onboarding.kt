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
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.isActive
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
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Warning
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
            else -> "Not connected. Please pair with your PC."
        }
        DiagnosticState.USB_UNAUTHORIZED -> "Waiting for UScreen desktop..."
        DiagnosticState.SOCKET_REFUSED -> "Connection refused. Is UScreen running on your PC?"
        DiagnosticState.WIFI_HOST_UNREACHABLE -> "Wi-Fi connected, but host unreachable. Are you on the same network?"
        DiagnosticState.CONNECTING -> "Waiting for UScreen desktop..."
    }
    return Pair(state, message)
}

/**
 * Status checklist row used in the troubleshooter.
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
                        color = if (isPassed) OnSurface else Muted
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

private enum class NoStream { WAITING, HOST_OFFLINE, DEBUGGING_DISABLED }

/**
 * The "waiting for host" screen: shown while no stream is live. There is no setup wizard; it only says
 * what is missing (cable, Wi-Fi, debugging) and offers a troubleshooter.
 */
@OptIn(ExperimentalAnimationApi::class, ExperimentalMaterial3Api::class, androidx.compose.material3.ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun WaitingScreen(
    videoReceiver: VideoReceiver?,
    @Suppress("UNUSED_PARAMETER") touchCapture: TouchCapture?,
    prefs: Prefs?,
    isConnected: Boolean,
    onOpenSettings: () -> Unit = {},
) {
    val context = LocalContext.current

    // Live status polling
    var devOptionsOn by remember { mutableStateOf(isDevOptionsEnabled(context)) }
    var usbAdbOn by remember { mutableStateOf(isUsbAdbEnabled(context)) }
    var wifiAdbOn by remember { mutableStateOf(isWifiAdbEnabled(context)) }
    var usbPlugged by remember { mutableStateOf(checkUsbConnected(context)) }
    var wifiConnected by remember { mutableStateOf(checkWifiConnected(context)) }
    var dynamicDiagnostic by remember { mutableStateOf<String?>(null) }
    var secondsWaiting by remember { mutableIntStateOf(0) }
    var hostShutdown by remember { mutableStateOf(videoReceiver?.hostShutdown == true) }
    // Result of the 1 Hz host probe below. Three failures in a row mean "Host Offline"; one success
    // means the host is up and we are only waiting for it to start the stream.
    var probeFailures by remember { mutableIntStateOf(0) }
    val preferredTransport = prefs?.preferredTransport ?: Prefs.TRANSPORT_AUTO

    LaunchedEffect(videoReceiver) {
        videoReceiver?.onDiagnosticUpdate = { diag ->
            (context as? ComponentActivity)?.runOnUiThread {
                dynamicDiagnostic = diag
            }
        }
    }

    LaunchedEffect(videoReceiver) {
        videoReceiver?.onHostShutdown = {
            (context as? ComponentActivity)?.runOnUiThread { hostShutdown = true }
        }
    }

    /*
     * Host reachability, at most once a second and only while the activity is STARTED (the loop is
     * cancelled on onStop and restarted on onStart, so a backgrounded app never polls). This is what
     * tells "Host Offline" (nobody answers) from "waiting for the host" (a listener answers, the
     * stream just has not started), and it brings the app back by itself when the daemon returns.
     */
    LaunchedEffect(videoReceiver) {
        val receiver = videoReceiver ?: return@LaunchedEffect
        val lifecycle = (context as? ComponentActivity)?.lifecycle ?: return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (isActive) {
                val up = probeHostSocket(receiver.host, receiver.port)
                if (up) {
                    if (probeFailures >= 3 || hostShutdown) {
                        // The host came back: leave Host Offline and restart the video socket.
                        hostShutdown = false
                        secondsWaiting = 0
                        receiver.retryConnection()
                    }
                    probeFailures = 0
                } else {
                    probeFailures += 1
                }
                delay(1000)
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

    /*
     * Which "nobody is streaming" state to show. A shutdown notice from the daemon wins outright. Otherwise,
     * after 5 s with USB debugging off the device itself is the problem; after the 10 s reconnect window
     * with a transport present and nothing answering, the daemon is not running.
     */
    val transportPresent = usbPlugged || wifiConnected
    val noStreamState = when {
        hostShutdown -> NoStream.HOST_OFFLINE
        probeFailures >= 3 && transportPresent -> NoStream.HOST_OFFLINE
        secondsWaiting > 5 && !usbAdbOn -> NoStream.DEBUGGING_DISABLED
        else -> NoStream.WAITING
    }


    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Surface0),
        contentAlignment = Alignment.Center
    ) {
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

            if (noStreamState == NoStream.HOST_OFFLINE) {
                Icon(
                    imageVector = Icons.Default.Error,
                    contentDescription = null,
                    tint = Color(0xFFEF5350),
                    modifier = Modifier.size(56.dp)
                )
                Spacer(modifier = Modifier.height(16.dp))
                Text("Host Offline", fontSize = 22.sp, fontWeight = FontWeight.Bold, color = Color(0xFFEF5350))
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = "The UScreen Pro daemon is not running on your PC. Please open UScreen Pro on your PC.",
                    fontSize = 14.sp, lineHeight = 20.sp, color = Muted, textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = 32.dp)
                )
                Spacer(modifier = Modifier.height(24.dp))
                Button(onClick = {
                    secondsWaiting = 0
                    probeFailures = 0
                    hostShutdown = false
                    videoReceiver?.retryConnection()
                }) { Text("Retry Connection") }
            } else if (noStreamState == NoStream.DEBUGGING_DISABLED) {
                Icon(
                    imageVector = Icons.Default.Warning,
                    contentDescription = null,
                    tint = Warn,
                    modifier = Modifier.size(56.dp)
                )
                Spacer(modifier = Modifier.height(16.dp))
                Text("Debugging Disabled", fontSize = 22.sp, fontWeight = FontWeight.Bold, color = Warn)
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = "Debugging is disabled. Please enable USB or Wireless Debugging.",
                    fontSize = 14.sp, lineHeight = 20.sp, color = Muted, textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = 32.dp)
                )
                Spacer(modifier = Modifier.height(24.dp))
                Button(onClick = {
                    try {
                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                    } catch (_: Exception) {
                        try { context.startActivity(Intent(Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } catch (_: Exception) {}
                    }
                }) { Text("Open Developer Options") }
            } else {
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
            when (diagState) {
                DiagnosticState.NO_TRANSPORT -> {
                    Spacer(modifier = Modifier.height(24.dp))
                    Button(onClick = onOpenSettings) { Text("Open Settings") }
                }
                DiagnosticState.SOCKET_REFUSED, DiagnosticState.WIFI_HOST_UNREACHABLE -> {
                    Spacer(modifier = Modifier.height(24.dp))
                    Button(onClick = {
                        secondsWaiting = 0
                        dynamicDiagnostic = null
                        videoReceiver?.retryConnection()
                    }) { Text("Retry") }
                }
                else -> {} // plain waiting: the spinner is enough
            }
            }
        }
    }
}
