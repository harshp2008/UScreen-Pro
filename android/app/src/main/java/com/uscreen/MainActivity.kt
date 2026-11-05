package com.uscreen

import android.app.KeyguardManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import android.util.Log
import android.view.SurfaceView
import android.view.View
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

class MainActivity : ComponentActivity() {
    /// Mirrors the host's mode so the UI can say what is going on.
    private var penOnlyMode by mutableStateOf(false)
    private var updateAvailable by mutableStateOf<String?>(null)
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

        // Start foreground service to prevent Samsung from killing us
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

            android.util.Log.i(
                "UScreen",
                "Requested display mode ${best.modeId} @ ${best.refreshRate}Hz " +
                    "(current ${disp.refreshRate}Hz)"
            )
        } catch (e: Exception) {
            android.util.Log.w("UScreen", "Could not request a refresh rate: ${e.message}")
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
        // Whether the panel is naturally landscape decides how the sensor's
        // angles read; see OrientationPolicy.
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
                        // Original APK behavior: default fullscreen without cutout takeover
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
            // Fallback: some Samsung firmwares have issues with insetsController
            android.util.Log.w("UScreen", "Immersive mode failed: ${e.message}")
        }
    }

    /**
     * Stylus hover, caught at the activity rather than on the SurfaceView.
     *
     * A hover listener on the SurfaceView does not reliably receive stylus
     * hover once Compose is in the picture — anything drawn over the surface
     * sits between the pointer and that listener. Hover is what moves the
     * cursor on the host, so without it there is no way to see where the pen
     * is pointing before it touches down, which matters most in graphics
     * tablet mode where the host's screen is all you are looking at.
     *
     * Safe to take at this level precisely because hover is not a click: it
     * cannot steal taps from the settings button the way intercepting touch
     * would.
     */
    /**
     * The daemon launches us with `am start --es token <hex>`; because the
     * activity is singleTask, a running app receives that here rather than
     * being recreated. A changed token means a new daemon run, so both
     * connections are torn down and rebuilt with it.
     */
    override fun onNewIntent(intent: android.content.Intent?) {
        super.onNewIntent(intent)
        intent?.let { applyIntentExtras(it, restart = true) }
    }

    private fun applyIntentExtras(intent: android.content.Intent, restart: Boolean) {
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
            // Re-assert once the window is actually attached: a request made in
            // onCreate can be dropped before the window exists.
            requestHighestRefreshRate()
        }
    }

    override fun onStart() {
        super.onStart()
        // One check per process, when the app comes to the front. It is a
        // single small request and the answer changes about once a month.
        if (!updateChecked) {
            updateChecked = true
            val cur = try { packageManager.getPackageInfo(packageName, 0).versionName ?: "0" } catch (_: Exception) { "0" }
            Thread {
                val found = UpdateCheck.newerThan(cur)
                if (found != null) runOnUiThread { updateAvailable = found }
            }.start()
        }
        // Connect control socket; mode acknowledgement from host starts videoReceiver if needed
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

private val Accent = Color(0xFF6C63FF)
private val AccentSoft = Color(0xFF8B85FF)
private val Ok = Color(0xFF4CAF50)
private val Warn = Color(0xFFFF9800)

@Composable
fun UScreenTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = Accent,
            secondary = Color(0xFF03DAC6),
            background = Color(0xFF0A0A0A),
            surface = Color(0xFF16161F),
            surfaceVariant = Color(0xFF20202C),
        )
    ) {
        content()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun UScreenMain(
    onSurfaceReady: (SurfaceView) -> Unit,
    penOnly: Boolean = false,
    updateAvailable: String? = null,
    showThanks: Boolean = false,
    onDismissThanks: () -> Unit = {},
    onSurfaceDestroyed: () -> Unit = {},
    videoReceiver: VideoReceiver? = null,
    touchCapture: TouchCapture? = null,
    onOrientationChange: (Int) -> Unit = {},
    prefs: Prefs? = null,
    isDeviceLocked: Boolean = false,
) {
    var isConnected by remember { mutableStateOf(false) }
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

    val context = LocalContext.current
    val activity = context as? MainActivity

    // Only intercept Back button when opt-in immersive mode is ON and streaming or pen mode.
    // When immersive mode is OFF (Default), Back button behaves as normal Android back (does nothing special).
    BackHandler(enabled = immersiveOn && (isConnected || penOnly) && !showSettings) {
        showSettings = true
    }

    // Re-assert immersive mode whenever settings sheet closes
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
        // Video surface — fills entire screen
        AndroidView(
            factory = { ctx ->
                SurfaceView(ctx).apply {
                    holder.setFormat(android.graphics.PixelFormat.OPAQUE)
                    holder.addCallback(
                        object : android.view.SurfaceHolder.Callback {
                            override fun surfaceCreated(holder: android.view.SurfaceHolder) {
                                onSurfaceReady(this@apply)
                            }
                            override fun surfaceChanged(
                                holder: android.view.SurfaceHolder,
                                format: Int, width: Int, height: Int
                            ) {
                                onSurfaceReady(this@apply)
                            }
                            override fun surfaceDestroyed(holder: android.view.SurfaceHolder) {
                                onSurfaceDestroyed()
                            }
                        }
                    )
                }
            },
            modifier = Modifier.fillMaxSize()
        )

        // Pen-only: there is no picture coming, so say so instead of leaving
        // the user staring at a "waiting for the host" spinner forever.
        AnimatedVisibility(
            visible = penOnly,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.fillMaxSize()
        ) {
            PenOnlyScreen(blankAfterSeconds = if (blankInPenMode) 8 else 0)
        }

        // Connection / State-Aware Onboarding screen
        AnimatedVisibility(
            visible = !isConnected && !penOnly,
            enter = fadeIn(),
            exit = fadeOut(),
            modifier = Modifier.fillMaxSize()
        ) {
            OnboardingScreen(
                videoReceiver = videoReceiver,
                touchCapture = touchCapture,
                prefs = prefs,
                isConnected = isConnected
            )
        }

        // Stats chip (top-left, only while streaming)
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
                    color = Color(0xFFB0B0C0),
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp)
                )
            }
        }

        // Device locked warning banner
        if (isDeviceLocked) {
            Surface(
                color = Color(0xD9B71C1C),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier
                    .align(Alignment.TopCenter)
                    .padding(top = 16.dp)
            ) {
                Text(
                    text = "🔒 Device Locked — Touch Disabled",
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = Color.White,
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 6.dp)
                )
            }
        }

        // Settings handle (top-right).
        // OFF (Default): Always visible, exactly matching the original APK.
        // ON (Opt-in): Hidden during streaming, visible when disconnected.
        val showSettingsHandle = !immersiveOn || !isConnected

        if (showSettingsHandle) {
            Box(
                modifier = Modifier
                    .align(Alignment.TopEnd)
                    .padding(10.dp)
                    .size(38.dp)
                    .alpha(if (penOnly && blankInPenMode) 0.18f else if (isConnected) 0.35f else 0.9f)
                    .clip(CircleShape)
                    .background(Color(0xAA20202C))
                    .clickable { showSettings = true },
                contentAlignment = Alignment.Center
            ) {
                Text("⚙", fontSize = 18.sp, color = Color.White)
            }
        }

        // "Update available" pill at bottom center so it does not overlap the top settings cog.
        // Only show on failure/waiting screen, never while actively streaming.
        if (updateAvailable != null && !isConnected && !penOnly && !showThanks) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 24.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background(Color(0xCC20202C))
                    .clickable {
                        context.startActivity(
                            Intent(
                                Intent.ACTION_VIEW,
                                android.net.Uri.parse(UpdateCheck.RELEASES_PAGE)
                            )
                        )
                    }
                    .padding(horizontal = 16.dp, vertical = 8.dp)
            ) {
                Text("Update $updateAvailable available", fontSize = 12.sp, color = Color.White)
            }
        }

        // One-time note after the first successful picture. Dismissable, never
        // repeated: the point is one honest ask, not a nag.
        if (showThanks) {
            Box(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(24.dp)
                    .clip(RoundedCornerShape(14.dp))
                    .background(Color(0xE620202C))
                    .padding(16.dp)
            ) {
                Column {
                    Text("UScreen is working.", fontSize = 15.sp, color = Color.White, fontWeight = FontWeight.Bold)
                    Text(
                        "If it replaced a second monitor for you, a star on GitHub or a compatibility " +
                            "report helps other Linux users find it. This note appears only once.",
                        fontSize = 12.sp, color = Color(0xFFB0B0C0)
                    )
                    Row(modifier = Modifier.padding(top = 10.dp)) {
                        Text("Open GitHub", fontSize = 13.sp, color = Accent,
                            modifier = Modifier.clickable {
                                context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW,
                                    android.net.Uri.parse("https://github.com/majmichu1/UScreen")))
                                onDismissThanks()
                            }.padding(end = 20.dp))
                        Text("Dismiss", fontSize = 13.sp, color = Color(0xFF9A9AB0),
                            modifier = Modifier.clickable { onDismissThanks() })
                    }
                }
            }
        }

        if (showSettings) {
            SettingsSheet(
                prefs = prefs,
                updateAvailable = updateAvailable,
                onOpenUpdate = {
                    context.startActivity(
                        android.content.Intent(
                            android.content.Intent.ACTION_VIEW,
                            android.net.Uri.parse(UpdateCheck.RELEASES_PAGE)
                        )
                    )
                },
                penOnly = penOnly,
                onPenOnlyChange = { wantPenOnly ->
                    // Fire and forget: the host answers with the mode it
                    // actually switched to, and that answer is what moves the
                    // UI. Flipping it here as well would show the new mode
                    // even when the host never got the message.
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
                    // The one moment the tablet speaks: the user chose these.
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
                },
                onExitApp = {
                    activity?.finish()
                },
                onDismiss = { showSettings = false }
            )
        }
    }
}

@Composable
private fun PenOnlyScreen(blankAfterSeconds: Int) {
    // After the explanation has been read the screen goes fully black: on an
    // OLED panel black pixels are off, and the explanation would otherwise sit
    // on screen for as long as the pen is in use.
    var blank by remember { mutableStateOf(false) }
    LaunchedEffect(blankAfterSeconds) {
        blank = false
        if (blankAfterSeconds > 0) {
            kotlinx.coroutines.delay(blankAfterSeconds * 1000L)
            blank = true
        }
    }
    if (blank) {
        Box(modifier = Modifier.fillMaxSize().background(Color.Black))
        return
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(
                Brush.verticalGradient(
                    listOf(Color(0xFF0D0D14), Color(0xFF141B2A), Color(0xFF0D0D14))
                )
            ),
        contentAlignment = Alignment.Center
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text("Graphics tablet", fontSize = 34.sp, fontWeight = FontWeight.Bold,
                color = Color.White)
            Spacer(Modifier.height(10.dp))
            Text(
                "Draw here — it goes to the screen on your computer.",
                fontSize = 15.sp, color = AccentSoft, textAlign = TextAlign.Center
            )
            Spacer(Modifier.height(28.dp))
            Text(
                "Nothing is streamed to this screen in this mode, so there is no\n" +
                    "display latency at all. Pressure, tilt and the eraser all work.",
                fontSize = 13.sp, lineHeight = 22.sp, color = Color(0xFF9A9AAE),
                textAlign = TextAlign.Center
            )
            if (blankAfterSeconds > 0) {
                Spacer(Modifier.height(18.dp))
                Text(
                    "The screen goes black in a few seconds to spare OLED panels. " +
                        "Settings are behind the faint ⚙ in the corner.",
                    fontSize = 12.sp, color = Color(0xFF6A6A7E), textAlign = TextAlign.Center
                )
            }
        }
    }
}

enum class OnboardingStep {
    DEVELOPER_OPTIONS_DISABLED,
    USB_DEBUGGING_DISABLED,
    WAITING_FOR_HOST
}

private fun getOnboardingStep(context: Context): OnboardingStep {
    val devEnabled = try {
        Settings.Global.getInt(context.contentResolver, Settings.Global.DEVELOPMENT_SETTINGS_ENABLED, 0) != 0
    } catch (_: Exception) {
        true
    }
    if (!devEnabled) return OnboardingStep.DEVELOPER_OPTIONS_DISABLED

    val adbEnabled = try {
        val usbAdb = Settings.Global.getInt(context.contentResolver, Settings.Global.ADB_ENABLED, 0) != 0
        val wifiAdb = Settings.Global.getInt(context.contentResolver, "adb_wifi_enabled", 0) != 0
        usbAdb || wifiAdb
    } catch (_: Exception) {
        true
    }
    if (!adbEnabled) return OnboardingStep.USB_DEBUGGING_DISABLED

    return OnboardingStep.WAITING_FOR_HOST
}

private fun checkUsbConnected(context: Context): Boolean {
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

private fun checkWifiConnected(context: Context): Boolean {
    val cm = context.getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager ?: return false
    val net = cm.activeNetwork ?: return false
    val caps = cm.getNetworkCapabilities(net) ?: return false
    return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)
}

enum class DiagnosticState {
    NO_TRANSPORT,
    USB_UNAUTHORIZED,
    SOCKET_REFUSED,
    WIFI_HOST_UNREACHABLE,
    CONNECTING
}

private fun evaluateDiagnostic(
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OnboardingScreen(
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

    // Refresh system status periodically
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

        // Interactive "Help Me Fix It" Wizard
        if (showHelpWizard) {
            val devOn = step != OnboardingStep.DEVELOPER_OPTIONS_DISABLED
            val adbOn = step == OnboardingStep.WAITING_FOR_HOST
            val transportOk = when (preferredTransport) {
                Prefs.TRANSPORT_USB -> usbPlugged
                Prefs.TRANSPORT_WIFI -> wifiOn
                else -> usbPlugged || wifiOn
            }

            // Trigger reconnection attempt as soon as all prerequisites become true
            LaunchedEffect(transportOk, devOn, adbOn) {
                if (transportOk && devOn && adbOn) {
                    videoReceiver?.restartSocket()
                    touchCapture?.connect()
                }
            }

            // Dismiss troubleshooter ONLY when connection is established
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
                            .verticalScroll(androidx.compose.foundation.rememberScrollState())
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

                        // Step 1: Transport check adapted to Preferred Transport
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

                        // Checklist Step 2: Developer Options
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

                        // Checklist Step 3: Debugging
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

@Composable
private fun TroubleshootCheckItem(
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
                color = if (passed) Color(0xFF4CAF50) else Color(0xFFEF5350),
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsSheet(
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

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = Color(0xFF16161F)
    ) {
        // Scrolls: on a landscape phone or a 600 px tablet the sheet is taller
        // than the screen, and everything below the first few settings was
        // unreachable (#27).
        Column(
            modifier = Modifier
                .verticalScroll(androidx.compose.foundation.rememberScrollState())
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

            // What the tablet is for, right now. Everything below only applies
            // when it is a screen, so the stream controls fold away when it
            // isn't.
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

            // Which way round the tablet is held. Applies at once, in both
            // modes, and needs no Apply: it is the tablet's own business, the
            // host never sees it. Automatic uses the tilt sensor directly;
            // with it off, the two pinned directions appear.
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
                // 120 is not offered: the generated EDID caps the virtual mode
                // at 90 Hz, so anything above would be duplicate frames.
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

            // OLED care for graphics-tablet mode. Applies at once.
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

            // Fingers or pen only. Applies at once; the host never sees it.
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

            // Opt-in immersive mode toggle.
            // OFF (Default): original APK UI with visible settings button and standard Back button behavior.
            // ON: full-screen immersive with hidden settings button and Back button mapping.
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

            // Keep connected in background
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

            // Keep display alive when locked
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
