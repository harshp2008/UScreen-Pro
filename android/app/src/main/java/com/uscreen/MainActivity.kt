/*
 * The app's single activity: lifecycle, video rendering, input dispatch, refresh-rate and
 * immersive-mode handling, and hosting the Compose UI.
 */
package com.uscreen

import android.app.KeyguardManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import android.view.View
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.appcompat.app.AppCompatActivity
import androidx.appcompat.app.AppCompatDelegate
import androidx.activity.compose.setContent
import androidx.compose.runtime.mutableStateOf
import com.uscreen.ui.UScreenMain
import com.uscreen.ui.UScreenTheme
import kotlin.math.roundToInt
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue

/**
 * Primary Android Activity managing lifecycle, hardware video rendering, low-latency touch/stylus
 * hover dispatch, screen refresh rate negotiation, and system immersive UI flags.
 */
class MainActivity : AppCompatActivity() {
    private var hostVersion by mutableStateOf<String?>(null)
    private var hostNotice by mutableStateOf<HostNotice?>(null)
    /** Set when the host forgot this device; survives restarts until the host talks to us again. */
    private var unpairedByHost by mutableStateOf(false)
    private var showThanks by mutableStateOf(false)
    private var videoReceiver: VideoReceiver? = null
    private var touchCapture: TouchCapture? = null
    private lateinit var prefs: Prefs
    private var wakeLock: PowerManager.WakeLock? = null
    private var isDeviceLocked by mutableStateOf(false)
    private var lockReceiver: BroadcastReceiver? = null

    /** The per-device settings the host pushed (or the saved copy until it does). */
    private var hostConfig by mutableStateOf(HostConfig())
    /** The host has been unreachable for the whole [ReconnectPolicy.OFFLINE_AFTER_MS] window. */
    private var hostOffline by mutableStateOf(false)
    /** Battery saver gave up on the host: no retry loop runs until the user taps Retry. */
    private var autoRetryDisabled by mutableStateOf(false)
    private val reconnectPolicy = ReconnectPolicy()
    private lateinit var dimmer: ScreenDimmer
    /** The host's idle dimming has the screen faded down right now (drives the wake-on-touch overlay). */
    private var dimmedByHost by mutableStateOf(false)
    /** Between onResume and onPause: the only time the app's own brightness override may be in force. */
    private var inForeground = false


    /**
     * Let the activity stay visible and wake the screen over the keyguard when the user opted into
     * "keep display alive when locked"; otherwise restore the default lock-screen behaviour.
     */
    fun applyLockScreenFlags() {
        val keepAlive = prefs.keepDisplayAliveWhenLocked
        setShowWhenLocked(keepAlive)
        setTurnScreenOn(keepAlive)
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

    /**
     * Takes a partial wake lock (up to 10 hours, not reference counted) so the CPU keeps running the sockets while the screen is off; failure is logged, not fatal.
     */
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

    /** Releases the wake lock if it is held; errors are ignored. */
    private fun releaseWakeLock() {
        try {
            wakeLock?.let { if (it.isHeld) it.release() }
        } catch (_: Exception) {}
    }

    /** Reads the keyguard state, dismisses an insecure keyguard, and re-applies the brightness policy. */
    private fun checkKeyguardState() {
        val km = getSystemService(Context.KEYGUARD_SERVICE) as KeyguardManager
        val locked = km.isKeyguardLocked
        val secure = km.isKeyguardSecure
        isDeviceLocked = locked
        if (locked && !secure) {
            km.requestDismissKeyguard(this, null)
        }
        applyBrightnessPolicy()
    }

    /**
     * What the window's brightness override should be right now. While the device is locked, or the app
     * is not in the foreground, no override at all: a screen dimmed to 5% would otherwise leave the PIN pad
     * invisible. Back on screen and unlocked, the dim the host asked for (or battery saver's 10%) returns
     * until the user touches the screen.
     */
    private fun applyBrightnessPolicy() {
        if (!::dimmer.isInitialized) return
        when {
            isDeviceLocked || !inForeground -> dimmer.restoreSystem()
            autoRetryDisabled -> dimmer.setNow(FadeCurve.BATTERY_SAVER_LEVEL)
            dimmedByHost -> dimmer.setNow(FadeCurve.DIM_LEVEL)
        }
    }

    /** The user touched the dimmed screen: fade back up with the host's undim curve and tell the host. */
    private fun wakeFromIdle() {
        if (!dimmedByHost) return
        dimmedByHost = false
        dimmer.undim(hostConfig.undimDurationMs, hostConfig.undimCurve)
        touchCapture?.sendWake()
    }

    /**
     * Listens for screen on/off and user-present broadcasts to re-check the keyguard; registered once.
     */
    private fun registerLockReceiver() {
        if (lockReceiver != null) return
        val receiver = object : BroadcastReceiver() {
            /** Re-checks the keyguard on screen off, screen on and user present. */
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

    /** Unregisters the lock receiver if registered. */
    private fun unregisterLockReceiver() {
        lockReceiver?.let {
            try { unregisterReceiver(it) } catch (_: Exception) {}
            lockReceiver = null
        }
    }

    /** Light / Dark / System choice. Read by the Compose theme, so a change repaints without a restart. */
    private var themeMode by mutableStateOf(Prefs.THEME_SYSTEM)

    /**
     * Apply the saved Light / Dark / System choice. AppCompatDelegate carries it to everything that is not
     * Compose (resources, dialogs, the window); Compose reads [themeMode] directly because the activity
     * handles uiMode itself and so never gets a recreate to pick the new configuration up.
     */
    fun applyThemeMode(mode: Int = prefs.themeMode) {
        themeMode = mode
        AppCompatDelegate.setDefaultNightMode(
            when (mode) {
                Prefs.THEME_LIGHT -> AppCompatDelegate.MODE_NIGHT_NO
                Prefs.THEME_DARK -> AppCompatDelegate.MODE_NIGHT_YES
                else -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM
            }
        )
    }

    /**
     * Creates the activity: the theme is applied before `super.onCreate` so the first frame is already in the chosen scheme, then the preferences, the content and the connections are set up.
     */
    override fun onCreate(savedInstanceState: Bundle?) {
        // Before super.onCreate, so the first frame is already in the chosen scheme.
        prefs = Prefs(this)
        applyThemeMode()
        super.onCreate(savedInstanceState)

        // Keep screen on while streaming
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        requestHighestRefreshRate()

        applyOrientation()
        applyLockScreenFlags()
        registerLockReceiver()
        checkKeyguardState()

        dimmer = ScreenDimmer({ window }, this)
        hostConfig = HostConfig.fromPrefs(prefs)
        videoReceiver = VideoReceiver()
        touchCapture = TouchCapture().also {
            it.setTouchEnabled(hostConfig.touchInput || hostConfig.inputMode == HostConfig.MODE_TOUCHPAD)
            it.setTouchpadMode(hostConfig.inputMode == HostConfig.MODE_TOUCHPAD)
            it.setStylusMode(hostConfig.inputMode == HostConfig.MODE_STYLUS)
            it.setTouchpadShape(hostConfig.touchpadShape)
            it.hostConfig = hostConfig
            // Touchpad clicks tick lightly when the setting is on (it is on by default).
            it.onHaptic = {
                if (hostConfig.touchpadHaptics && hostConfig.inputMode == HostConfig.MODE_TOUCHPAD) Haptics.click(this)
            }
        }
        touchCapture?.onHostConfig = { cfg -> runOnUiThread { applyHostConfig(cfg) } }
        touchCapture?.onDimCommand = { dim, durationMs, curve ->
            runOnUiThread {
                dimmedByHost = dim
                // Locked or backgrounded: remember the state, but leave the system brightness alone.
                if (isDeviceLocked || !inForeground) applyBrightnessPolicy()
                else if (dim) dimmer.dim(durationMs, curve) else dimmer.undim(durationMs, curve)
            }
        }
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
                    val wasRunning = hostConfig.streamsVideo
                    if (wasRunning) vr.stop()
                    vr.mimeType = mime
                    if (wasRunning) vr.start()
                }
            }
        }
        touchCapture?.onHostNotice = { notice ->
            runOnUiThread { hostNotice = notice }
        }
        unpairedByHost = prefs.unpairedByHost
        touchCapture?.onUnpaired = {
            prefs.unpairedByHost = true
            runOnUiThread { unpairedByHost = true }
        }
        // A greeting from the host means we are paired again (or still), so the banner goes away.
        touchCapture?.onPairedGreeting = {
            if (prefs.unpairedByHost) {
                prefs.unpairedByHost = false
                runOnUiThread { unpairedByHost = false }
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
            UScreenTheme(themeMode = themeMode) {
                UScreenMain(
                    penOnly = hostConfig.inputMode == HostConfig.MODE_STYLUS,
                    showThanks = showThanks,
                    onDismissThanks = { showThanks = false },
                    videoReceiver = videoReceiver,
                    touchCapture = touchCapture,
                    prefs = prefs,
                    isDeviceLocked = isDeviceLocked,
                    hostVersion = hostVersion,
                    hostNotice = hostNotice,
                    unpairedByHost = unpairedByHost,
                    hostConfig = hostConfig,
                    hostOffline = hostOffline,
                    autoRetryDisabled = autoRetryDisabled,
                    onRetry = { retryHostLink() },
                    idleDimmed = dimmedByHost,
                    onWake = { wakeFromIdle() },
                    onSetting = { key, value -> onUserSetting(key, value) },
                    onSurfaceReady = { surfaceView ->
                        videoReceiver?.setSurface(surfaceView)
                        touchCapture?.setSurfaceView(surfaceView)
                    },
                    onSurfaceDestroyed = { videoReceiver?.onSurfaceDestroyed() },
                    onStreamingChange = { live ->
                        streaming = live
                        applyOrientation()
                    },
                )
            }
        }

        startLinkWatchdog()

        // Start foreground service to prevent system kill
        val serviceIntent = Intent(this, StreamingService::class.java)
        startForegroundService(serviceIntent)

        // Enable fullscreen AFTER setContent so DecorView exists
        window.decorView.post {
            applyImmersiveMode()
        }
    }

    /**
     * Mirror the host's per-device settings into [Prefs], the live input handling and the UI. Runs on the
     * UI thread; the host sends it on connect and whenever a setting changes there (or here).
     */
    private fun applyHostConfig(cfg: HostConfig) {
        val prev = hostConfig
        hostConfig = cfg
        touchCapture?.hostConfig = cfg
        prefs.inputMode = cfg.inputMode
        prefs.immersiveModeEnabled = cfg.immersiveMode
        prefs.showStats = cfg.showStats
        prefs.touchEnabled = cfg.touchInput
        prefs.touchOnLockScreen = cfg.touchOnLockScreen
        prefs.keepDisplayAliveWhenLocked = cfg.keepDisplayAliveWhenLocked
        prefs.keepConnectedInBackground = cfg.keepConnectedInBackground
        prefs.blankPenOnly = cfg.blankPenOnly
        prefs.orientation = Prefs.orientationFromName(cfg.orientation)
        prefs.themeMode = Prefs.themeFromName(cfg.themeMode)
        prefs.batterySaverOnDisconnect = cfg.batterySaverOnDisconnect
        prefs.batterySaverLock = cfg.batterySaverLock
        prefs.wifiConfigured = cfg.wifiConfigured
        prefs.touchpadHaptics = cfg.touchpadHaptics
        prefs.nativeGestures = cfg.nativeGestures
        prefs.touchpadSensitivity = cfg.touchpadSensitivity
        prefs.rightClickMethod = cfg.rightClickMethod
        prefs.touchpadShape = cfg.touchpadShape
        prefs.stylusStyle = cfg.stylusStyle
        prefs.stylusButtonPrimary = cfg.stylusButtonPrimary
        prefs.stylusButtonSecondary = cfg.stylusButtonSecondary
        prefs.penMenu = cfg.penMenu
        prefs.penMenuStyle = cfg.penMenuStyle
        prefs.penMenuHoldMs = cfg.penMenuHoldMs
        prefs.stylusIdleHideTimeout = cfg.stylusIdleHideTimeout
        // Touchpad mode is made of finger input, so it overrides the "Touch input" switch.
        touchCapture?.setTouchEnabled(cfg.touchInput || cfg.inputMode == HostConfig.MODE_TOUCHPAD)
        touchCapture?.setTouchpadMode(cfg.inputMode == HostConfig.MODE_TOUCHPAD)
        touchCapture?.setStylusMode(cfg.inputMode == HostConfig.MODE_STYLUS)
        touchCapture?.setTouchpadShape(cfg.touchpadShape)
        touchCapture?.refreshNativeGestures()
        if (prev.immersiveMode != cfg.immersiveMode) applyImmersiveMode(cfg.immersiveMode)
        if (prev.keepDisplayAliveWhenLocked != cfg.keepDisplayAliveWhenLocked) applyLockScreenFlags()
        if (prev.orientation != cfg.orientation) applyOrientation()
        if (prev.themeMode != cfg.themeMode) applyThemeMode(prefs.themeMode)
        // Only Display mode has a video stream: the host stops capturing for the other two, so the
        // decoder has nothing to wait for. start() and stop() are idempotent.
        if (!autoRetryDisabled) {
            if (cfg.streamsVideo) videoReceiver?.start() else videoReceiver?.stop()
        }
        Log.i("UScreen", "Host config applied: $cfg")
    }

    /**
     * A synced setting was changed on this tablet: apply it now and tell the host to store it as this
     * device's override. [value] is a Boolean, an Int or a String, as the key requires.
     */
    private fun onUserSetting(key: String, value: Any) {
        val next = hostConfig.withSetting(key, value) ?: return
        applyHostConfig(next)
        touchCapture?.sendSetting(key, value)
        // "Require Device Admin Lock" needs the permission; ask for it right away.
        if (key == "battery_saver_lock" && value == true && !ScreenLock.canLock(this)) {
            try { startActivity(ScreenLock.requestIntent(this)) } catch (_: Exception) {}
        }
    }

    /**
     * Watches the control socket while the activity is visible. Every 2 s of failure is just another
     * retry (the socket and video loops do the retrying); only a link that stayed down for the whole
     * 120 s window flips [hostOffline] and, with battery saver on, shuts the app down for the night.
     */
    private fun startLinkWatchdog() {
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                reconnectPolicy.linkUp()
                while (isActive) {
                    val now = SystemClock.elapsedRealtime()
                    if (touchCapture?.isControlConnected() == true) {
                        reconnectPolicy.linkUp()
                        if (hostOffline && !autoRetryDisabled) hostOffline = false
                    } else if (!autoRetryDisabled) {
                        reconnectPolicy.linkDown(now)
                        // A screen faded down for idleness is no use while the host is gone.
                        if (dimmedByHost) {
                            dimmedByHost = false
                            dimmer.undim(500, "ease_in")
                        }
                        if (reconnectPolicy.expired(now) && !hostOffline) {
                            Log.i("UScreen", "Host unreachable for ${ReconnectPolicy.OFFLINE_AFTER_MS / 1000} s")
                            hostOffline = true
                            if (hostConfig.batterySaverOnDisconnect) enterBatterySaver()
                        }
                    }
                    delay(1000)
                }
            }
        }
    }

    /**
     * Battery saver after the reconnect window: stop every retry loop, drop to 10% brightness and let go of
     * the wake lock and keep-screen-on flag. Locking is separate and optional ("Require Device Admin
     * Lock"): only with it on, and the Device Admin permission granted, is `lockNow()` called.
     */
    private fun enterBatterySaver() {
        Log.i("UScreen", "Battery saver: stopping retries and dimming (lock: ${hostConfig.batterySaverLock})")
        autoRetryDisabled = true
        touchCapture?.autoReconnect = false
        touchCapture?.disconnect()
        videoReceiver?.autoRetry = false
        dimmedByHost = false
        applyBrightnessPolicy()
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        releaseWakeLock()
        if (hostConfig.batterySaverLock) {
            setShowWhenLocked(false)
            setTurnScreenOn(false)
            if (!ScreenLock.lockNow(this)) {
                Log.w("UScreen", "Device Admin not granted — not locking")
            }
        }
    }

    /** The Retry button, or the host coming back: re-arm every loop and reconnect now. */
    private fun retryHostLink() {
        Log.i("UScreen", "Retrying the host link")
        autoRetryDisabled = false
        hostOffline = false
        reconnectPolicy.linkUp()
        dimmedByHost = false
        dimmer.restoreSystem()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        applyLockScreenFlags()
        touchCapture?.autoReconnect = true
        touchCapture?.connect()
        videoReceiver?.retryConnection()
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

    /** True while video is flowing; only then is the picture pinned to landscape. */
    private var streaming = false

    /**
     * Sets the screen orientation: free while nothing streams (onboarding follows the device); once video flows, landscape with the camera up, camera down, or following the tilt sensor, per the setting.
     */
    private fun applyOrientation() {
        if (!streaming) {
            // Onboarding and the waiting screen follow the device, so portrait works too.
            stopTiltListener()
            requestedOrientation = android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
            return
        }
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

    /**
     * Starts the tilt listener that switches between the two landscape orientations by sensor; does nothing when it is running or the sensor cannot detect orientation.
     */
    @Suppress("DEPRECATION")
    private fun startTiltListener() {
        if (tiltListener != null) return
        val display = windowManager.defaultDisplay
        val size = android.graphics.Point().also { display.getRealSize(it) }
        val naturalLandscape = OrientationPolicy.naturallyLandscape(size.x, size.y, display.rotation)
        tiltListener = object : android.view.OrientationEventListener(this) {
            /** Requests the landscape orientation that matches the sensor reading. */
            override fun onOrientationChanged(orientation: Int) {
                val want = OrientationPolicy.landscapeForSensor(orientation, naturalLandscape) ?: return
                if (requestedOrientation != want) requestedOrientation = want
            }
        }.also { if (it.canDetectOrientation()) it.enable() else tiltListener = null }
    }

    /** Stops the tilt listener. */
    private fun stopTiltListener() {
        tiltListener?.disable()
        tiltListener = null
    }

    /** Pauses: not in the foreground any more, so the brightness override is given back to the system. */
    override fun onPause() {
        super.onPause()
        // Never leave a brightness override behind when the window is not the one in front.
        inForeground = false
        if (::dimmer.isInitialized) dimmer.restoreSystem()
    }

    /** Resumes: foreground again, re-checks the keyguard, re-applies the layout and pings the host. */
    override fun onResume() {
        super.onResume()
        inForeground = true
        checkKeyguardState()
        applyImmersiveMode()
        touchCapture?.sendPing()
    }

    /**
     * Switch between the default window insets and the opt-in immersive layout (content under the
     * cutout, system bars hidden). Wrapped in try/catch because OEM window managers differ.
     */
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

    /**
     * A new launch intent (the host starts the app with the token, ports and host address as extras): applies them and restarts the connections.
     */
    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        intent?.let { applyIntentExtras(it, restart = true) }
    }

    /**
     * Applies the extras of the launch intent: session token, video and input ports and the host address; reconnects when the ports or the host changed and [restart] is true.
     */
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
            if (hostConfig.streamsVideo) {
                videoReceiver?.stop()
                videoReceiver?.start()
            }
        }
    }

    /** Forwards stylus hover to the input link; other motion events are left to the system. */
    override fun onGenericMotionEvent(event: android.view.MotionEvent): Boolean {
        val w = window.decorView.width
        val h = window.decorView.height
        if (w > 0 && h > 0 && touchCapture?.handleHoverEvent(event, w, h) == true) {
            return true
        }
        return super.onGenericMotionEvent(event)
    }

    /** On window focus: re-applies the immersive layout and asks for the highest refresh rate. */
    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            applyImmersiveMode()
            requestHighestRefreshRate()
        }
    }

    /** Connects the input link and pings the host. */
    override fun onStart() {
        super.onStart()
        touchCapture?.connect()
        touchCapture?.sendPing()
    }

    /**
     * Stops: gives the brightness back and, unless `keepConnectedInBackground` is set, closes the video and input connections.
     */
    override fun onStop() {
        super.onStop()
        inForeground = false
        if (::dimmer.isInitialized) dimmer.restoreSystem()
        if (!prefs.keepConnectedInBackground) {
            Log.i("UScreen", "App backgrounded and keepConnectedInBackground is false — disconnecting")
            videoReceiver?.stop()
            touchCapture?.disconnect()
        }
    }

    /**
     * Destroys the activity: releases brightness, tilt listener, receiver and wake lock, and stops the streaming service.
     */
    override fun onDestroy() {
        super.onDestroy()
        dimmer.restoreSystem()
        stopTiltListener()
        unregisterLockReceiver()
        releaseWakeLock()
        stopService(Intent(this, StreamingService::class.java))
    }
}
