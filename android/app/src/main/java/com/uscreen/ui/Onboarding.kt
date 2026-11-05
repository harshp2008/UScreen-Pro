package com.uscreen.ui

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uscreen.Prefs
import com.uscreen.TouchCapture
import com.uscreen.VideoReceiver
import kotlinx.coroutines.delay

/**
 * High-level onboarding prerequisites progression state.
 */
enum class OnboardingStep {
    DEVELOPER_OPTIONS_DISABLED,
    USB_DEBUGGING_DISABLED,
    WAITING_FOR_HOST
}

/**
 * Query Android system settings to evaluate if Developer Options and USB/Wireless
 * debugging are enabled.
 */
fun getOnboardingStep(context: Context): OnboardingStep {
    val devEnabled = try {
        Settings.Global.getInt(
            context.contentResolver,
            Settings.Global.DEVELOPMENT_SETTINGS_ENABLED,
            0
        ) != 0
    } catch (_: Exception) {
        true
    }
    if (!devEnabled) return OnboardingStep.DEVELOPER_OPTIONS_DISABLED

    val adbEnabled = try {
        val usbAdb = Settings.Global.getInt(
            context.contentResolver,
            Settings.Global.ADB_ENABLED,
            0
        ) != 0
        val wifiAdb = Settings.Global.getInt(
            context.contentResolver,
            "adb_wifi_enabled",
            0
        ) != 0
        usbAdb || wifiAdb
    } catch (_: Exception) {
        true
    }
    if (!adbEnabled) return OnboardingStep.USB_DEBUGGING_DISABLED

    return OnboardingStep.WAITING_FOR_HOST
}

/**
 * Check whether a physical USB cable connection is currently established with a host workstation.
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
 * Check whether the device is connected to a local Wi-Fi access point.
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
 * Interactive checklist row inside the Troubleshooter Bottom Sheet.
 * Displays step name, description, pass/fail status indicator, and optional action button.
 */
@Composable
fun TroubleshootCheckItem(
    title: String,
    detail: String,
    passed: Boolean,
    actionButtonText: String? = null,
    onActionClick: (() -> Unit)? = null
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(Color(0x332A2A3D))
            .padding(12.dp)
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth()
        ) {
            Text(
                text = if (passed) "✓" else "✗",
                fontSize = 18.sp,
                fontWeight = FontWeight.Bold,
                color = if (passed) Ok else Color(0xFFEF5350),
                modifier = Modifier.padding(end = 12.dp)
            )
            Column(modifier = Modifier.weight(1f)) {
                Text(title, fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = Color.White)
                Text(detail, fontSize = 12.sp, color = Color(0xFFB0B0C0))
            }
        }
        if (!passed && actionButtonText != null && onActionClick != null) {
            Spacer(Modifier.height(8.dp))
            Button(
                onClick = onActionClick,
                colors = ButtonDefaults.buttonColors(containerColor = Accent),
                modifier = Modifier.padding(start = 28.dp)
            ) {
                Text(actionButtonText, fontSize = 12.sp, color = Color.White, fontWeight = FontWeight.SemiBold)
            }
        }
    }
}

/**
 * Primary onboarding and connection waiting screen.
 *
 * Guides the user through unlocking Developer Options, enabling USB/Wireless debugging,
 * monitoring connection progress, and launching the interactive Troubleshooter wizard.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OnboardingScreen(
    videoReceiver: VideoReceiver?,
    touchCapture: TouchCapture?,
    prefs: Prefs?,
    isConnected: Boolean
) {
    val context = LocalContext.current
    var step by remember { mutableStateOf(getOnboardingStep(context)) }
    var usbPlugged by remember { mutableStateOf(checkUsbConnected(context)) }
    var wifiOn by remember { mutableStateOf(checkWifiConnected(context)) }
    var showHelpWizard by remember { mutableStateOf(false) }
    var dynamicDiagnostic by remember { mutableStateOf<String?>(null) }
    var secondsWaiting by remember { mutableStateOf(0) }
    val preferredTransport = prefs?.preferredTransport ?: Prefs.TRANSPORT_AUTO

    LaunchedEffect(videoReceiver) {
        videoReceiver?.onDiagnosticUpdate = { diag ->
            (context as? ComponentActivity)?.runOnUiThread {
                dynamicDiagnostic = diag
            }
        }
    }

    LaunchedEffect(Unit) {
        while (true) {
            step = getOnboardingStep(context)
            usbPlugged = checkUsbConnected(context)
            wifiOn = checkWifiConnected(context)
            secondsWaiting += 1
            delay(1000)
        }
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(Color(0xFF0D0D14), Color(0xFF14142A), Color(0xFF0D0D14))
                )
            ),
        contentAlignment = Alignment.Center
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.padding(24.dp)
        ) {
            Text(
                text = "UScreen",
                fontSize = 42.sp,
                fontWeight = FontWeight.Bold,
                color = Color.White
            )
            Text(
                text = "USB & Wireless Display",
                fontSize = 15.sp,
                color = AccentSoft
            )
            Spacer(modifier = Modifier.height(32.dp))

            when (step) {
                OnboardingStep.DEVELOPER_OPTIONS_DISABLED -> {
                    Card(
                        shape = RoundedCornerShape(16.dp),
                        colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
                        modifier = Modifier.fillMaxWidth(0.85f)
                    ) {
                        Column(
                            modifier = Modifier.padding(24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            Text(
                                text = "Developer Options Required",
                                fontSize = 18.sp,
                                color = Warn,
                                fontWeight = FontWeight.Bold,
                                textAlign = TextAlign.Center
                            )
                            Spacer(modifier = Modifier.height(10.dp))
                            Text(
                                text = "UScreen communicates with your PC via Android USB/Wi-Fi debugging. " +
                                    "Tap 'Build number' 7 times in About Device to unlock Developer options.",
                                fontSize = 13.sp,
                                lineHeight = 20.sp,
                                color = Color(0xFFB0B0C0),
                                textAlign = TextAlign.Center
                            )
                            Spacer(modifier = Modifier.height(18.dp))
                            Button(
                                onClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_DEVICE_INFO_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                },
                                colors = ButtonDefaults.buttonColors(containerColor = Accent)
                            ) {
                                Text("Open About Device", color = Color.White, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                }
                OnboardingStep.USB_DEBUGGING_DISABLED -> {
                    Card(
                        shape = RoundedCornerShape(16.dp),
                        colors = CardDefaults.cardColors(containerColor = Color(0x8C1A1A2A)),
                        modifier = Modifier.fillMaxWidth(0.85f)
                    ) {
                        Column(
                            modifier = Modifier.padding(24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally
                        ) {
                            Text(
                                text = "Enable USB / Wireless Debugging",
                                fontSize = 18.sp,
                                color = Warn,
                                fontWeight = FontWeight.Bold,
                                textAlign = TextAlign.Center
                            )
                            Spacer(modifier = Modifier.height(10.dp))
                            Text(
                                text = "Developer options are enabled, but debugging is turned off. " +
                                    "Please turn on 'USB debugging' or 'Wireless debugging' to allow your PC to connect.",
                                fontSize = 13.sp,
                                lineHeight = 20.sp,
                                color = Color(0xFFB0B0C0),
                                textAlign = TextAlign.Center
                            )
                            Spacer(modifier = Modifier.height(18.dp))
                            Button(
                                onClick = {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                },
                                colors = ButtonDefaults.buttonColors(containerColor = Accent)
                            ) {
                                Text("Open Developer Options", color = Color.White, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                }
                OnboardingStep.WAITING_FOR_HOST -> {
                    val (diagState, diagMessage) = evaluateDiagnostic(
                        usbPlugged = usbPlugged,
                        wifiOn = wifiOn,
                        preferredTransport = preferredTransport,
                        dynamicDiagnostic = dynamicDiagnostic,
                        secondsWaiting = secondsWaiting
                    )

                    CircularProgressIndicator(
                        color = Accent,
                        strokeWidth = 4.dp,
                        modifier = Modifier.size(48.dp)
                    )
                    Spacer(modifier = Modifier.height(24.dp))
                    Text(
                        text = diagMessage,
                        fontSize = 14.sp,
                        lineHeight = 20.sp,
                        color = if (diagState == DiagnosticState.CONNECTING) Color(0xFFB0B0C0) else Warn,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.padding(horizontal = 32.dp)
                    )
                    Spacer(modifier = Modifier.height(28.dp))
                    OutlinedButton(
                        onClick = { showHelpWizard = true },
                        colors = ButtonDefaults.outlinedButtonColors(contentColor = AccentSoft)
                    ) {
                        Text("Help Me Fix It 🛠", fontSize = 13.sp)
                    }
                }
            }
        }

        if (showHelpWizard) {
            val devOn = step != OnboardingStep.DEVELOPER_OPTIONS_DISABLED
            val adbOn = step == OnboardingStep.WAITING_FOR_HOST
            val transportOk = when (preferredTransport) {
                Prefs.TRANSPORT_USB -> usbPlugged
                Prefs.TRANSPORT_WIFI -> wifiOn
                else -> usbPlugged || wifiOn
            }

            LaunchedEffect(transportOk, devOn, adbOn) {
                if (transportOk && devOn && adbOn) {
                    videoReceiver?.restartSocket()
                    touchCapture?.connect()
                }
            }

            LaunchedEffect(isConnected) {
                if (isConnected) {
                    showHelpWizard = false
                }
            }

            ModalBottomSheet(
                onDismissRequest = { showHelpWizard = false },
                containerColor = Color(0xFF16161F)
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
                            color = Color.White
                        )
                        Spacer(Modifier.height(8.dp))
                        Text(
                            "Let's verify each requirement step-by-step:",
                            fontSize = 13.sp,
                            color = Color(0xFF9A9AB0)
                        )
                        Spacer(Modifier.height(16.dp))

                        val (tTitle, tDetail) = when (preferredTransport) {
                            Prefs.TRANSPORT_USB -> "1. USB Cable Connection" to (
                                if (usbPlugged) "USB cable is securely detected." else "No USB cable detected. Please connect your USB cable."
                            )
                            Prefs.TRANSPORT_WIFI -> "1. Wi-Fi Connection" to (
                                if (wifiOn) "Connected to Wi-Fi network." else "Wi-Fi is disconnected. Please connect to Wi-Fi."
                            )
                            else -> "1. Network Transport" to when {
                                usbPlugged && wifiOn -> "USB and Wi-Fi connected."
                                usbPlugged -> "USB cable is securely detected."
                                wifiOn -> "Connected to Wi-Fi network."
                                else -> "No USB or Wi-Fi connection detected. Connect cable or Wi-Fi."
                            }
                        }

                        TroubleshootCheckItem(
                            title = tTitle,
                            detail = tDetail,
                            passed = transportOk,
                            actionButtonText = if (!transportOk && preferredTransport == Prefs.TRANSPORT_WIFI) "Open Wi-Fi Settings" else null,
                            onActionClick = if (!transportOk && preferredTransport == Prefs.TRANSPORT_WIFI) {
                                {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                }
                            } else null
                        )
                        Spacer(Modifier.height(12.dp))

                        TroubleshootCheckItem(
                            title = "2. Developer Options",
                            detail = if (devOn) "Developer options enabled." else "Disabled. Must tap Build number 7 times in About Device.",
                            passed = devOn,
                            actionButtonText = if (!devOn) "Open About Device" else null,
                            onActionClick = if (!devOn) {
                                {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_DEVICE_INFO_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                }
                            } else null
                        )
                        Spacer(Modifier.height(12.dp))

                        TroubleshootCheckItem(
                            title = "3. USB / Wireless Debugging",
                            detail = if (adbOn) "Debugging is enabled." else "Disabled. Must toggle USB or Wireless debugging in Developer Options.",
                            passed = adbOn,
                            actionButtonText = if (!adbOn) "Open Developer Options" else null,
                            onActionClick = if (!adbOn) {
                                {
                                    try {
                                        context.startActivity(Intent(Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS))
                                    } catch (_: Exception) {
                                        context.startActivity(Intent(Settings.ACTION_SETTINGS))
                                    }
                                }
                            } else null
                        )

                        Spacer(Modifier.height(20.dp))
                        Text(
                            text = if (transportOk && devOn && adbOn) "All checks passed! Connecting to PC..." else "Once all items show a green checkmark, UScreen will connect automatically.",
                            fontSize = 12.sp,
                            color = if (transportOk && devOn && adbOn) AccentSoft else Color(0xFF9A9AB0),
                            textAlign = TextAlign.Center,
                            modifier = Modifier.fillMaxWidth()
                        )
                        Spacer(Modifier.height(16.dp))
                    }
                }
            }
        }
    }
}
