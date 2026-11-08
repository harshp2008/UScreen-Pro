package com.uscreen.ui

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
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
import androidx.compose.animation.ExperimentalAnimationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.Prefs
import com.uscreen.ReconnectPolicy
import com.uscreen.TouchCapture
import com.uscreen.VideoReceiver
import kotlinx.coroutines.delay
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue

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
    /** True once the host has been unreachable for the whole reconnect window (2 minutes). */
    hostOffline: Boolean = false,
    /** Battery saver gave up: nothing retries by itself until [onRetry]. */
    autoRetryDisabled: Boolean = false,
    onRetry: () -> Unit = {},
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
    val preferredTransport = prefs?.preferredTransport ?: Prefs.TRANSPORT_AUTO

    LaunchedEffect(videoReceiver) {
        videoReceiver?.onDiagnosticUpdate = { diag ->
            (context as? ComponentActivity)?.runOnUiThread {
                dynamicDiagnostic = diag
            }
        }
    }

    /*
     * Host reachability, once every retry interval (2 s) and only while the activity is STARTED (the
     * loop is cancelled on onStop and restarted on onStart, so a backgrounded app never polls). It
     * brings the app back by itself when the daemon returns after "Host Offline" was shown. When battery
     * saver has given up (autoRetryDisabled) it stays silent: only the Retry button reconnects.
     */
    LaunchedEffect(videoReceiver, hostOffline, autoRetryDisabled) {
        val receiver = videoReceiver ?: return@LaunchedEffect
        if (!hostOffline || autoRetryDisabled) return@LaunchedEffect
        val lifecycle = (context as? ComponentActivity)?.lifecycle ?: return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (isActive) {
                if (probeHostSocket(receiver.host, receiver.port)) {
                    secondsWaiting = 0
                    onRetry()
                }
                delay(ReconnectPolicy.RETRY_INTERVAL_MS)
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
     * Which "nobody is streaming" state to show. "Host Offline" is shown only once the activity's link watchdog
     * has seen the host unreachable for the full 120 s reconnect window; until then the app keeps retrying
     * every 2 s behind the ordinary waiting screen. After 5 s with USB debugging off the device itself is the
     * problem.
     */
    val noStreamState = when {
        hostOffline -> NoStream.HOST_OFFLINE
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
                    text = if (autoRetryDisabled)
                        "The UScreen Pro daemon has been unreachable for 2 minutes. Battery saver stopped retrying; tap Retry once it is running."
                    else
                        "The UScreen Pro daemon is not running on your PC. Please open UScreen Pro on your PC.",
                    fontSize = 14.sp, lineHeight = 20.sp, color = Muted, textAlign = TextAlign.Center,
                    modifier = Modifier.padding(horizontal = 32.dp)
                )
                Spacer(modifier = Modifier.height(24.dp))
                Button(onClick = {
                    secondsWaiting = 0
                    onRetry()
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
