package com.uscreen

import android.app.KeyguardManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.util.Log
import android.view.View
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.uscreen.ui.UScreenMain
import com.uscreen.ui.UScreenTheme
import java.net.Inet4Address
import java.net.NetworkInterface
import java.util.Collections
import kotlin.math.roundToInt

/**
 * Retrieve the active IPv4 address assigned to the device's wireless (`wlan`) interface.
 */
fun getLocalWifiIp(@Suppress("UNUSED_PARAMETER") context: Context? = null): String? {
    try {
        val interfaces = Collections.list(NetworkInterface.getNetworkInterfaces())
        for (intf in interfaces) {
            if (intf.name.startsWith("wlan")) {
                for (addr in Collections.list(intf.inetAddresses)) {
                    if (!addr.isLoopbackAddress && addr is Inet4Address) {
                        return addr.hostAddress
                    }
                }
            }
        }
    } catch (_: Exception) {}
    return null
}

/**
 * Primary Android Activity managing lifecycle, hardware video rendering, low-latency touch/stylus
 * hover dispatch, screen refresh rate negotiation, and system immersive UI flags.
 */
class MainActivity : ComponentActivity() {
    /// Mirrors the host's mode so the UI can say what is going on.
    private var penOnlyMode by mutableStateOf(false)
    private var updateAvailable by mutableStateOf<String?>(null)
    private var hostVersion by mutableStateOf<String?>(null)
    private var updateChecked = false
    private var showThanks by mutableStateOf(false)
    private var videoReceiver: VideoReceiver? = null
    private var touchCapture: TouchCapture? = null
    private lateinit var prefs: Prefs
    private var wakeLock: PowerManager.WakeLock? = null
    private var isDeviceLocked by mutableStateOf(false)
    private var lockReceiver: BroadcastReceiver? = null

    fun applyLockScreenFlags() {
        val keepAlive = prefs.keepDisplayAliveWhenLocked
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            setShowWhenLocked(keepAlive)
            setTurnScreenOn(keepAlive)
        }
        if (keepAlive) {
            @Suppress("DEPRECATION")
            window.addFlags(
                WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON
            )
            acquireWakeLock()
        } else {
            @Suppress("DEPRECATION")
            window.clearFlags(
                WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON
            )
            releaseWakeLock()
        }
    }

    private fun acquireWakeLock() {
        if (wakeLock == null) {
            val pm = getSystemService(Context.POWER_SERVICE) as PowerManager
            wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "UScreen:KeepAliveWakeLock").apply {
                setReferenceCounted(false)
            }
        }
        try {
            wakeLock?.let { if (!it.isHeld) it.acquire(10 * 60 * 60 * 1000L) }
        } catch (e: Exception) {
            Log.w("UScreen", "Could not acquire WakeLock: ${e.message}")
        }
    }

    private fun releaseWakeLock() {
        try {
            wakeLock?.let { if (it.isHeld) it.release() }
        } catch (_: Exception) {}
    }

    private fun checkKeyguardState() {
        val km = getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        val locked = km.isKeyguardLocked
        val secure = km.isKeyguardSecure
        isDeviceLocked = locked
        if (locked && !secure) {
            km.requestDismissKeyguard(this, null)
        }
    }

    private fun registerLockReceiver() {
        if (lockReceiver != null) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: Intent?) {
                when (intent?.action) {
                    Intent.ACTION_SCREEN_OFF,
                    Intent.ACTION_SCREEN_ON,
                    Intent.ACTION_USER_PRESENT -> {
                        checkKeyguardState()
                    }
                }
            }
        }
        val filter = IntentFilter().apply {
            addAction(Intent.ACTION_SCREEN_OFF)
            addAction(Intent.ACTION_SCREEN_ON)
            addAction(Intent.ACTION_USER_PRESENT)
        }
        registerReceiver(receiver, filter)
        lockReceiver = receiver
    }

    private fun unregisterLockReceiver() {
        lockReceiver?.let {
            try { unregisterReceiver(it) } catch (_: Exception) {}
            lockReceiver = null
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Keep screen on while streaming
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        requestHighestRefreshRate()

        prefs = Prefs(this)
        applyOrientation()
        applyLockScreenFlags()
        registerLockReceiver()
        checkKeyguardState()

        videoReceiver = VideoReceiver()
        touchCapture = TouchCapture().also { it.setTouchEnabled(prefs.touchEnabled) }
        applyIntentExtras(intent, restart = false)

        // Close the host's latency measurement loop: every acknowledged frame
        // lets the host time capture→display on its own clock.
        videoReceiver?.onFrameRendered = { seq, decodeUs ->
            touchCapture?.sendRendered(seq, decodeUs)
            // First frame ever on screen: say thanks once, then never again.
            if (!prefs.thankedOnce) {
                prefs.thankedOnce = true
                runOnUiThread { showThanks = true }
            }
        }
        videoReceiver?.streamFps = prefs.fps
        // The computer's own fps and bitrate, sent on connect and whenever they
        // change there. The settings sheet shows these, and nothing is sent
        // back until the user presses Apply: the tablet used to re-send its
        // remembered copy on every connect and overwrite config.toml (#37).
        touchCapture?.onSettingsKnown = { fps, kbps ->
            prefs.fps = fps
            prefs.bitrateKbps = kbps
            videoReceiver?.streamFps = fps
        }
        touchCapture?.onCodecKnown = { codec ->
            val mime = if (codec == "hevc") VideoReceiver.MIME_TYPE_HEVC
                       else VideoReceiver.MIME_TYPE
            val vr = videoReceiver
            if (vr != null && vr.mimeType != mime) {
                runOnUiThread {
                    // The decoder is built once per streaming session, so a
                    // codec change has to restart it. This normally fires
                    // before the first frame and costs nothing; it only
                    // restarts anything if the host switched codec while
                    // connected.
                    Log.i("UScreen", "Host is sending $codec — rebuilding the decoder")
                    val wasRunning = !penOnlyMode
                    if (wasRunning) vr.stop()
                    vr.mimeType = mime
                    if (wasRunning) vr.start()
                }
            }
        }
        touchCapture?.onModeKnown = { penOnly ->
            runOnUiThread {
                penOnlyMode = penOnly
                if (penOnly) videoReceiver?.stop() else videoReceiver?.start()
            }
        }
        touchCapture?.onHostVersionKnown = { hostVer ->
            runOnUiThread {
                hostVersion = hostVer
            }
        }
        touchCapture?.currentPreferredTransport = when (prefs.preferredTransport) {
            Prefs.TRANSPORT_WIFI -> "wifi"
            Prefs.TRANSPORT_USB -> "usb"
            else -> "auto"
        }

        // Report the real screen size (landscape-oriented) so the host can
        // size the virtual display to match this tablet exactly.
        @Suppress("DEPRECATION")
        val size = android.graphics.Point().also {
            windowManager.defaultDisplay.getRealSize(it)
        }
        if (size.x > 0 && size.y > 0) {
            val w = maxOf(size.x, size.y)
            val h = minOf(size.x, size.y)
            // Physical size too: the host puts it in the EDID, and the desktop
            // derives its DPI — and therefore its default scale — from that.
            // Paired long-edge-to-long-edge so it matches the w/h above
            // regardless of the panel's natural orientation.
            val dm = resources.displayMetrics
            val mmA = if (dm.xdpi > 1f) size.x / dm.xdpi * 25.4f else 0f
            val mmB = if (dm.ydpi > 1f) size.y / dm.ydpi * 25.4f else 0f
            val wMm = maxOf(mmA, mmB).roundToInt()
            val hMm = minOf(mmA, mmB).roundToInt()
            touchCapture?.setNativeResolution(w, h, wMm, hMm)
            videoReceiver?.formatWidth = w
            videoReceiver?.formatHeight = h
        }

        setContent {
            UScreenTheme {
                UScreenMain(
                    penOnly = penOnlyMode,
                    updateAvailable = updateAvailable,
                    showThanks = showThanks,
                    onDismissThanks = { showThanks = false },
                    videoReceiver = videoReceiver,
                    touchCapture = touchCapture,
                    prefs = prefs,
                    isDeviceLocked = isDeviceLocked,
                    hostVersion = hostVersion,
                    onSurfaceReady = { surfaceView ->
                        videoReceiver?.setSurface(surfaceView)
                        touchCapture?.setSurfaceView(surfaceView)
                    },
                    onSurfaceDestroyed = { videoReceiver?.onSurfaceDestroyed() },
                    onOrientationChange = { choice ->
                        prefs.orientation = choice
                        applyOrientation()
                    }
                )
            }
        }

        // Start foreground service to prevent system kill
        val serviceIntent = Intent(this, StreamingService::class.java)
        startForegroundService(serviceIntent)

        // Enable fullscreen AFTER setContent so DecorView exists
        window.decorView.post {
            applyImmersiveMode()
        }
    }

    /**
     * Ask for the panel's fastest mode.
     *
     * Refresh rate is a direct latency cost, not just a smoothness one: a
     * decoded frame waits for the next vsync before it is visible, so 60Hz adds
     * up to 16.7ms (~8ms on average) versus 8.3ms (~4ms) at 120Hz. Measured
     * decode+render was ~14.8ms with the panel at 60Hz.
     *
     * This can only ask. Samsung's "Motion smoothness: Standard" setting
     * (`secure refresh_rate_mode = 0`) caps the panel at 60Hz system-wide and
     * overrides any app request — switching it to Adaptive is the user's call,
     * and is worth more latency than any change on the host side.
     */
    private fun requestHighestRefreshRate() {
        try {
            @Suppress("DEPRECATION")
            val disp = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display
                else windowManager.defaultDisplay
            val best = disp?.supportedModes?.maxByOrNull { it.refreshRate } ?: return

            // Reassigning the same LayoutParams instance can be ignored, so
            // apply the change through an explicit set.
            val lp = window.attributes
            lp.preferredDisplayModeId = best.modeId
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                lp.preferredRefreshRate = best.refreshRate
            }
            window.attributes = lp

            Log.i(
                "UScreen",
                "Requested display mode ${best.modeId} @ ${best.refreshRate}Hz " +
                    "(current ${disp.refreshRate}Hz)"
            )
        } catch (e: Exception) {
            Log.w("UScreen", "Could not request a refresh rate: ${e.message}")
        }
    }

    /**
     * Which way round the tablet is held.
     *
     * The manifest's sensorLandscape ought to flip between the two landscape
     * directions on its own, but on the reference tablet (Tab S9 Ultra, One
     * UI, auto-rotate on) it never left "camera up". So the automatic mode
     * reads the tilt sensor itself and pins the direction explicitly, which
     * the system does honour; the manual modes pin it and ignore the sensor.
     */
    private var tiltListener: android.view.OrientationEventListener? = null

    private fun applyOrientation() {
        when (prefs.orientation) {
            Prefs.ORIENTATION_CAMERA_UP -> {
                stopTiltListener()
                requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
            }
            Prefs.ORIENTATION_CAMERA_DOWN -> {
                stopTiltListener()
                requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_REVERSE_LANDSCAPE
            }
            else -> {
                requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                startTiltListener()
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun startTiltListener() {
        if (tiltListener != null) return
        val display = windowManager.defaultDisplay
        val size = android.graphics.Point().also { display.getRealSize(it) }
        val naturalLandscape = OrientationPolicy.naturallyLandscape(size.x, size.y, display.rotation)
        tiltListener = object : android.view.OrientationEventListener(this) {
            override fun onOrientationChanged(orientation: Int) {
                val want = OrientationPolicy.landscapeForSensor(orientation, naturalLandscape) ?: return
                if (requestedOrientation != want) requestedOrientation = want
            }
        }.also { if (it.canDetectOrientation()) it.enable() else tiltListener = null }
    }

    private fun stopTiltListener() {
        tiltListener?.disable()
        tiltListener = null
    }

    override fun onResume() {
        super.onResume()
        applyImmersiveMode()
        touchCapture?.sendPing()
    }

    fun applyImmersiveMode(optIn: Boolean = prefs.immersiveModeEnabled) {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                val lp = window.attributes
                if (optIn) {
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                        lp.layoutInDisplayCutoutMode =
                            WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                    } else {
                        lp.layoutInDisplayCutoutMode =
                            WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                    }
                } else {
                    lp.layoutInDisplayCutoutMode =
                        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT
                }
                window.attributes = lp
            }

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                window.setDecorFitsSystemWindows(false)
                window.insetsController?.let { controller ->
                    if (optIn) {
                        controller.hide(android.view.WindowInsets.Type.systemBars())
                    } else {
                        controller.hide(
                            android.view.WindowInsets.Type.statusBars() or
                            android.view.WindowInsets.Type.navigationBars()
                        )
                    }
                    controller.systemBarsBehavior =
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                }
            } else {
                @Suppress("DEPRECATION")
                window.decorView.systemUiVisibility = (
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                    View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                )
            }
        } catch (e: Exception) {
            Log.w("UScreen", "Immersive mode failed: ${e.message}")
        }
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        intent?.let { applyIntentExtras(it, restart = true) }
    }

    private fun applyIntentExtras(intent: Intent, restart: Boolean) {
        val fromIntent = intent.getStringExtra("token")
        if (fromIntent != null) prefs.hostToken = fromIntent
        val token = prefs.hostToken

        val newVideoPort = intent.getIntExtra("video_port", -1)
        val newInputPort = intent.getIntExtra("input_port", -1)
        val newHost = intent.getStringExtra("host")

        var portChanged = false
        var hostChanged = false

        if (newHost != null && newHost.isNotBlank()) {
            if (touchCapture?.host != newHost || videoReceiver?.host != newHost) {
                Log.i("UScreen", "Assigned host IP from intent: $newHost")
                touchCapture?.host = newHost
                videoReceiver?.host = newHost
                hostChanged = true
            }
        }

        if (newVideoPort > 0 && videoReceiver?.port != newVideoPort) {
            Log.i("UScreen", "Assigned unique video port: $newVideoPort")
            videoReceiver?.port = newVideoPort
            portChanged = true
        }
        if (newInputPort > 0 && touchCapture?.port != newInputPort) {
            Log.i("UScreen", "Assigned unique input port: $newInputPort")
            touchCapture?.port = newInputPort
            portChanged = true
        }

        if (token == null) return
        val tokenChanged = touchCapture?.token != token
        touchCapture?.token = token
        videoReceiver?.token = token

        // Only rebuild live connections if token, ports, or host changed
        if (restart && (tokenChanged || portChanged || hostChanged) && touchCapture?.isControlConnected() == true) {
            Log.i("UScreen", "Session parameters updated — reconnecting")
            touchCapture?.disconnect()
            touchCapture?.connect()
            if (!penOnlyMode) {
                videoReceiver?.stop()
                videoReceiver?.start()
            }
        }
    }

    override fun onGenericMotionEvent(event: android.view.MotionEvent): Boolean {
        val w = window.decorView.width
        val h = window.decorView.height
        if (w > 0 && h > 0 && touchCapture?.handleHoverEvent(event, w, h) == true) {
            return true
        }
        return super.onGenericMotionEvent(event)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            applyImmersiveMode()
            requestHighestRefreshRate()
        }
    }

    override fun onStart() {
        super.onStart()
        if (!updateChecked) {
            updateChecked = true
            val cur = try { packageManager.getPackageInfo(packageName, 0).versionName ?: "0" } catch (_: Exception) { "0" }
            Thread {
                val found = UpdateCheck.newerThan(cur)
                if (found != null) runOnUiThread { updateAvailable = found }
            }.start()
        }
        touchCapture?.connect()
        touchCapture?.sendPing()
    }

    override fun onStop() {
        super.onStop()
        if (!prefs.keepConnectedInBackground) {
            Log.i("UScreen", "App backgrounded and keepConnectedInBackground is false — disconnecting")
            videoReceiver?.stop()
            touchCapture?.disconnect()
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        stopTiltListener()
        unregisterLockReceiver()
        releaseWakeLock()
        stopService(Intent(this, StreamingService::class.java))
    }
}
