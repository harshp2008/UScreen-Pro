package com.uscreen

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.MotionEvent
import android.view.SurfaceView
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin
import kotlinx.coroutines.*
import okhttp3.*
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * WebSocket client for the host's input server (loopback port 8891, reached through `adb reverse`).
 *
 * It authenticates with the session token, sends the display handshake (native resolution and
 * physical size), and translates Android [MotionEvent]s into the JSON touch / pen / mouse messages
 * the host injects through uinput. It also receives host notices (pause, settings, update) and
 * reconnects with a fixed delay when the socket drops.
 */
class TouchCapture {
    companion object {
        const val TAG = "UScreenTouch"
        const val DEFAULT_PORT = 8891
        // MotionEvent.TOOL_TYPE_PALM is @hide, so it has to be spelled out;
        // it is 5 in AOSP (0 unknown, 1 finger, 2 stylus, 3 mouse, 4 eraser,
        // 5 palm). It was 6 here until 1.2.5, so no palm was ever filtered.
        private const val TOOL_TYPE_PALM = 5

        /** Pointer speed: pixels sent to the computer per pixel of finger travel. */
        private const val PAD_GAIN = 1.6f

        /** The host's virtual touchpad surface in millimetres (see PAD_WIDTH_MM in uinput.rs). */
        private const val NATIVE_PAD_W_MM = 640f
        private const val NATIVE_PAD_H_MM = 420f

        /** How long the pen may be out of range before a held side button is let go. */
        private const val PEN_BUTTON_GRACE_MS = 250L

        /** A typical 11 inch tablet, used until the host tells us the real panel size. */
        private const val DEFAULT_MM_PER_PX = 0.0927f

        /** A tap in the click strip must be this short and stay this still to be a right click. */
        private const val ZONE_TAP_MS = 300L
        private const val ZONE_TAP_SLOP_MM = 3f

        /** `adb logcat -s UScreenInput` shows every palm, tap and gesture decision. */
        private const val DEBUG_INPUT = true
    }

    /** WebSocket port on 127.0.0.1. Defaults to 8891. */
    @Volatile var port: Int = DEFAULT_PORT

    /** Host IP address for WebSocket connection. Defaults to 127.0.0.1 (ADB reverse forward). */
    @Volatile var host: String = "127.0.0.1"

    /** Target display ID (defaults to 0 or virtual display ID). */
    @Volatile var targetDisplayId: Int = 0

    /** Virtual display resolution used for coordinate mapping. */
    @Volatile var virtualWidth: Int = 0
    @Volatile var virtualHeight: Int = 0

    private var webSocket: WebSocket? = null
    @Volatile private var isConnected = false
    private var reconnectJob: Job? = null
    private var surfaceView: SurfaceView? = null

    /** Set from the host's greeting: it is using us as a graphics tablet for
     *  its own screen, so no video will arrive and none should be waited for. */
    @Volatile var isPenOnly = false
        private set
    var onModeKnown: ((penOnly: Boolean) -> Unit)? = null
    var onCodecKnown: ((codec: String) -> Unit)? = null
    /** The computer's fps and bitrate, on connect and whenever they change there. */
    var onSettingsKnown: ((fps: Int, bitrateKbps: Int) -> Unit)? = null
    /** The computer's daemon Git version string, received on connect */
    var onHostVersionKnown: ((version: String) -> Unit)? = null
    /** Host-pushed state that needs an overlay: paused, settings updating, app updating. null clears it. */
    var onHostNotice: ((HostNotice?) -> Unit)? = null
    /** The host pushed its per-device settings ({"type":"config_update"}); [hostConfig] is already updated. */
    var onHostConfig: ((HostConfig) -> Unit)? = null
    /** The latest settings the host pushed, or the saved copy until it does. */
    @Volatile var hostConfig = HostConfig()
    /** Idle dimming from the host: `dim` true fades down, false fades back up. Durations in ms. */
    var onDimCommand: ((dim: Boolean, durationMs: Long, curve: String) -> Unit)? = null
    /** A touchpad click happened and the tablet should vibrate; set by the activity, which owns the vibrator. */
    var onHaptic: (() -> Unit)? = null
    /** The host forgot this device ({"type":"unpaired"}). */
    var onUnpaired: (() -> Unit)? = null
    /** The host sent a normal greeting/mode message: this app is paired. */
    var onPairedGreeting: (() -> Unit)? = null

    @Volatile var currentPreferredTransport: String = "auto"

    /**
     * Session token from the host, delivered as an intent extra when the
     * daemon launches us over adb. Must be the first thing sent on the
     * socket; without it the host closes the connection unanswered.
     */
    @Volatile var token: String? = null

    /**
     * Only settings the host has not yet accepted; guarded by this instance's monitor. A change
     * the WebSocket already took is not replayed on reconnect, so a stale tablet value can never
     * overwrite newer settings made on the computer (config.toml or the GUI).
     */
    private var pendingConfig: JSONObject? = null
    @Volatile private var pendingMode: JSONObject? = null

    /** Tablet's native landscape resolution, reported to the host on connect
     *  so the virtual display can match it automatically. */
    @Volatile private var nativeWidth = 0
    @Volatile private var nativeHeight = 0

    /** Physical panel size in millimetres, so the host can build an EDID that
     *  reports the true DPI and the desktop comes up at a sane scale. */
    @Volatile private var nativeWidthMm = 0
    @Volatile private var nativeHeightMm = 0

    /**
     * Record the panel's real pixel size and, when known, physical size in millimetres; the host uses
     * them to build a matching virtual display. The virtual size defaults to the native size until the
     * host answers with the mode it actually created.
     */
    fun setNativeResolution(width: Int, height: Int, widthMm: Int = 0, heightMm: Int = 0) {
        nativeWidth = width
        nativeHeight = height
        nativeWidthMm = widthMm
        nativeHeightMm = heightMm
        if (virtualWidth == 0) virtualWidth = width
        if (virtualHeight == 0) virtualHeight = height
    }

    private val client = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.SECONDS)
        .connectTimeout(5, TimeUnit.SECONDS)
        .build()

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    private val wsListener = object : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            isConnected = true
            // Anything still in force is replayed by the host right after the greeting.
            onHostNotice?.invoke(null)
            Log.i(TAG, "Connected")
            // Authenticate before anything else. If we have no token yet the
            // host will drop us and relaunch the app with one, and the
            // reconnect logic takes it from there.
            token?.let { t ->
                webSocket.send(JSONObject().apply {
                    put("type", "auth")
                    put("token", t)
                }.toString())
            } ?: Log.w(TAG, "No session token yet — the host will send one")
            if (nativeWidth > 0 && nativeHeight > 0) {
                val res = JSONObject().apply {
                    put("type", "resolution")
                    put("version", BuildConfig.VERSION_NAME)
                    put("width", nativeWidth)
                    put("height", nativeHeight)
                    if (nativeWidthMm > 0 && nativeHeightMm > 0) {
                        put("width_mm", nativeWidthMm)
                        put("height_mm", nativeHeightMm)
                    }
                    put("preferred_transport", currentPreferredTransport)
                }
                webSocket.send(res.toString())
                Log.i(TAG, "Reported native resolution: ${nativeWidth}x${nativeHeight} " +
                        "(${nativeWidthMm}x${nativeHeightMm} mm), version: ${BuildConfig.VERSION_NAME}")
            }
            sendPendingConfig(webSocket)
            pendingMode?.let {
                webSocket.send(it.toString())
                pendingMode = null
            }
            webSocket.send(JSONObject().apply {
                put("type", "ping")
            }.toString())
            Log.i(TAG, "Sent initial connection ping")
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            // The host greets with its mode; everything else it might say is
            // ignored, this channel is otherwise ours to talk on.
            try {
                val o = JSONObject(text)
                when (o.optString("type")) {
                    "paused" -> { onHostNotice?.invoke(HostNotice(HostNotice.Kind.PAUSED)); return }
                    "resumed" -> { onHostNotice?.invoke(null); return }
                    "settings_updating" -> { onHostNotice?.invoke(HostNotice(HostNotice.Kind.SETTINGS)); return }
                    "settings_applied" -> { onHostNotice?.invoke(null); return }
                    "updating" -> {
                        onHostNotice?.invoke(HostNotice(HostNotice.Kind.UPDATING, o.optString("host"), o.optString("device")))
                        return
                    }
                    "update_done" -> { onHostNotice?.invoke(null); return }
                    "unpaired" -> { onUnpaired?.invoke(); return }
                    "config_update" -> {
                        val next = HostConfig.fromJson(o, hostConfig)
                        hostConfig = next
                        onPairedGreeting?.invoke()
                        onHostConfig?.invoke(next)
                        return
                    }
                    "dim_screen" -> {
                        onDimCommand?.invoke(true, o.optLong("duration", 3000), o.optString("curve", "linear"))
                        return
                    }
                    "undim_screen" -> {
                        onDimCommand?.invoke(false, o.optLong("duration", 500), o.optString("curve", "linear"))
                        return
                    }
                }
                if (o.has("codec") || o.has("host_version") || o.has("pen_only")) onPairedGreeting?.invoke()
                if (o.has("codec")) {
                    onCodecKnown?.invoke(o.getString("codec"))
                }
                if (o.has("host_version")) {
                    val hv = o.getString("host_version")
                    onHostVersionKnown?.invoke(hv)
                }
                if (o.has("fps") && o.has("bitrate_kbps")) {
                    onSettingsKnown?.invoke(o.getInt("fps"), o.getInt("bitrate_kbps"))
                }
                if (o.has("pen_only")) {
                    val pen = o.getBoolean("pen_only")
                    if (pen != isPenOnly) {
                        isPenOnly = pen
                        Log.i(TAG, "Host mode: ${if (pen) "pen-only" else "display"}")
                    }
                    onModeKnown?.invoke(pen)
                }
            } catch (_: Exception) {}
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(1000, null)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            isConnected = false
            onHostNotice?.invoke(null)
            scheduleReconnect()
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            isConnected = false
            onHostNotice?.invoke(null)
            Log.w(TAG, "Connection failed: ${t.message}")
            scheduleReconnect()
        }
    }

    // The surface only forwards touches to the host; there is no click to perform.
    /**
     * Attach the view whose touches are forwarded to the host and remember which display it is on.
     */
    @android.annotation.SuppressLint("ClickableViewAccessibility")
    fun setSurfaceView(sv: SurfaceView) {
        surfaceView = sv
        try {
            targetDisplayId = sv.display?.displayId ?: 0
        } catch (_: Exception) {}

        sv.setOnTouchListener { view, event ->
            if (inputBlocked) {
                liftTouches()
                return@setOnTouchListener true
            }
            // Deliver touches as the panel reports them. Batched to the screen refresh, Android
            // resamples the newest point ahead of the real ones, which reaches the computer as
            // alternating tiny and large steps; unbuffered, a touchpad sees the stream a touchpad would.
            if (touchpadMode && event.actionMasked == MotionEvent.ACTION_DOWN) {
                view.requestUnbufferedDispatch(event)
            }
            handleMotionEvent(event, view.width, view.height)
            true
        }

        // S-Pen hover: pen near screen moves cursor without clicking.
        // Without this, the first touch always snaps the cursor to the pen
        // position and fires a click simultaneously (jarring).
        sv.setOnHoverListener { view, event ->
            if (!isConnected || inputBlocked) return@setOnHoverListener false
            val vw = view.width.coerceAtLeast(1).toFloat()
            val vh = view.height.coerceAtLeast(1).toFloat()
            when (event.actionMasked) {
                MotionEvent.ACTION_HOVER_ENTER,
                MotionEvent.ACTION_HOVER_MOVE -> {
                    if (isPenLike(event, 0)) {
                        notePenActivity()
                        sendPenEvent(event, 0, 3, vw, vh)
                    }
                }
                MotionEvent.ACTION_HOVER_EXIT -> {
                    if (isPenLike(event, 0)) sendPenProximityExit()
                }
            }
            true
        }
    }

    /**
     * Open the input WebSocket (idempotent) and start the authenticate-then-handshake sequence.
     */
    fun connect() {
        // Battery saver gave up on the host: onStart / onResume (which the lock screen triggers) must
        // not quietly bring the link back; only the Retry button does, by setting autoReconnect first.
        if (!autoReconnect) return
        penButtonDown = false
        // Idempotent: a second connect() must not leave the first socket
        // alive with its listener still flipping isConnected. onStart and a
        // token delivered through onNewIntent can both call this.
        if (isConnected) return
        // A reconnect already scheduled by onClosed would open a second
        // socket next to this one; this call supersedes it.
        reconnectJob?.cancel()
        webSocket?.cancel()
        webSocket = null
        connectWebSocket()
    }

    private fun connectWebSocket() {
        webSocket?.cancel()
        val request = Request.Builder()
            .url("ws://$host:$port")
            .build()
        webSocket = client.newWebSocket(request, wsListener)
    }

    /**
     * False while battery saver has given up on the host: a dropped socket then stays dropped until
     * [connect] is called again (the Retry button).
     */
    @Volatile var autoReconnect = true

    private fun scheduleReconnect() {
        if (!autoReconnect) return
        reconnectJob?.cancel()
        reconnectJob = scope.launch {
            delay(ReconnectPolicy.RETRY_INTERVAL_MS)
            if (!isConnected && autoReconnect) {
                connectWebSocket()
            }
        }
    }

    /**
     * Forward stylus hover so the host's cursor follows the pen before it
     * touches down. Returns true only for pen hover, so nothing else the
     * activity might want to do with generic motion events is disturbed.
     */
    fun handleHoverEvent(event: MotionEvent, width: Int, height: Int): Boolean {
        if (!isConnected) return false
        val vw = width.coerceAtLeast(1).toFloat()
        val vh = height.coerceAtLeast(1).toFloat()
        return when (event.actionMasked) {
            MotionEvent.ACTION_HOVER_ENTER,
            MotionEvent.ACTION_HOVER_MOVE -> {
                if (!isPenLike(event, 0)) return false
                mainHandler.removeCallbacks(releasePenButtons)
                sendPenEvent(event, 0, 3, vw, vh)
                true
            }
            MotionEvent.ACTION_HOVER_EXIT -> {
                if (!isPenLike(event, 0)) return false
                sendPenProximityExit()
                true
            }
            MotionEvent.ACTION_BUTTON_PRESS,
            MotionEvent.ACTION_BUTTON_RELEASE -> {
                if (!isPenLike(event, 0)) return false
                syncPenButton(event)
                true
            }
            else -> false
        }
    }

    private data class CoordinateQuad(val w: Float, val h: Float, val ox: Float, val oy: Float)

    /**
     * Maps physical view touch coordinates to the virtual display's normalized [0.0, 1.0] range.
     * Accounts for pillarbox/letterbox padding when panel aspect ratio diverges from virtual display.
     */
    fun mapCoordinates(rawX: Float, rawY: Float, vw: Float, vh: Float): Pair<Float, Float> {
        if (virtualWidth <= 0 || virtualHeight <= 0) {
            return (rawX / vw).coerceIn(0f, 1f) to (rawY / vh).coerceIn(0f, 1f)
        }
        val viewAspect = vw / vh
        val virtAspect = virtualWidth.toFloat() / virtualHeight.toFloat()
        val quad = if (viewAspect > virtAspect) {
            val h = vh
            val w = vh * virtAspect
            val ox = (vw - w) / 2f
            CoordinateQuad(w, h, ox, 0f)
        } else {
            val w = vw
            val h = vw / virtAspect
            val oy = (vh - h) / 2f
            CoordinateQuad(w, h, 0f, oy)
        }
        val nx = ((rawX - quad.ox) / quad.w).coerceIn(0f, 1f)
        val ny = ((rawY - quad.oy) / quad.h).coerceIn(0f, 1f)
        return nx to ny
    }

    /**
     * Translate one [MotionEvent] into host messages.
     *
     * Palms are dropped, pens and fingers are routed to separate message types (a pen in range
     * suppresses finger touches), and all coordinates are normalised against the given view size.
     * Returns false when the socket is not connected so the caller can fall through.
     */
    fun handleMotionEvent(event: MotionEvent, width: Int, height: Int): Boolean {
        if (!isConnected) return false

        val vw = width.coerceAtLeast(1).toFloat()
        val vh = height.coerceAtLeast(1).toFloat()

        val padBounds = TouchpadShape.bounds(touchpadShape, vw, vh)
        val pointerCount = event.pointerCount
        val actionIndex = event.actionIndex
        val maskedAction = event.actionMasked

        when (maskedAction) {
            MotionEvent.ACTION_DOWN,
            MotionEvent.ACTION_POINTER_DOWN -> {
                // Drop palm contacts — Samsung sends TOOL_TYPE_PALM for
                // unintentional palm-rest touches; forwarding them causes
                // phantom scrolling on the Linux side.
                if (isPalm(event, actionIndex)) {
                    return true
                }
                if (isPenLike(event, actionIndex)) {
                    notePenActivity()
                    sendPenEvent(event, actionIndex, 0, vw, vh)
                } else {
                    // Stylus mode is a graphics tablet: only the pen and its eraser count.
                    if (stylusMode || penIsActive() || !touchEnabled) return true
                    if (touchpadMode) {
                        // A finger that lands outside the touch area is ignored for its whole life:
                        // nothing is sent and no gesture sees it.
                        val id = event.getPointerId(actionIndex)
                        val x = event.getX(actionIndex)
                        val y = event.getY(actionIndex)
                        if (!padBounds.contains(x, y)) {
                            outsideIds.add(id)
                            return true
                        }
                        outsideIds.remove(id)
                        // The virtual touchpad's own motion shows the cursor; the host's 1 px nudge
                        // is only for the engine path.
                        if (maskedAction == MotionEvent.ACTION_DOWN && !nativeActive()) sendWake(force = false)
                        if (nativeActive()) feedRaw(event, padBounds, -1, actionIndex)
                        else feedPad(event, padBounds, -1)
                        return true
                    }
                    val slot = slotOf(event, actionIndex)
                    downSlots.add(slot)
                    val (nx, ny) = mapCoordinates(event.getX(actionIndex), event.getY(actionIndex), vw, vh)
                    sendTouch(nx, ny, event.getPressure(actionIndex).toDouble(), 0, slot)
                }
            }

            MotionEvent.ACTION_MOVE -> {
                var sawPadFinger = false
                for (i in 0 until pointerCount) {
                    if (isPalm(event, i)) {
                        // A finger that Android reclassifies as a palm mid-
                        // gesture never gets an UP we would forward, so lift
                        // it now or the Linux side keeps that slot pressed.
                        val slot = slotOf(event, i)
                        if (downSlots.remove(slot)) {
                            val (nx, ny) = mapCoordinates(event.getX(i), event.getY(i), vw, vh)
                            sendTouch(nx, ny, 0.0, 1, slot)
                        }
                        continue
                    }
                    if (isPenLike(event, i)) {
                        notePenActivity()
                        // Android batches several samples between frames.
                        // Forward the historical points too, otherwise fast
                        // pen strokes look jagged in GIMP.
                        val hist = event.historySize
                        for (h in 0 until hist) {
                            val (hx, hy) = mapCoordinates(event.getHistoricalX(i, h), event.getHistoricalY(i, h), vw, vh)
                            val hp = event.getHistoricalPressure(i, h).toDouble()
                            val (htx, hty) = decomposeTilt(
                                getHistoricalAxis(event, MotionEvent.AXIS_TILT, i, h),
                                getHistoricalAxis(event, MotionEvent.AXIS_ORIENTATION, i, h))
                            emitPen(hx.toDouble(), hy.toDouble(), hp, htx, hty,
                                isEraser(event, i), 2)
                        }
                        sendPenEvent(event, i, 2, vw, vh)
                    } else {
                        if (stylusMode || penIsActive() || !touchEnabled) continue
                        if (touchpadMode) {
                            sawPadFinger = true
                            continue
                        }
                        val (nx, ny) = mapCoordinates(event.getX(i), event.getY(i), vw, vh)
                        sendTouch(nx, ny, event.getPressure(i).toDouble(), 2, slotOf(event, i))
                    }
                }
                // One frame per sample Android batched, so the engine sees the same stream a pad would.
                if (sawPadFinger) {
                    if (nativeActive()) feedRaw(event, padBounds, -1, -1) else feedPad(event, padBounds, -1)
                }
            }

            // S-Pen side button. Fired as a discrete event while hovering or
            // drawing; forwarded as the stylus button (right-click in GIMP).
            MotionEvent.ACTION_BUTTON_PRESS -> {
                if (isPenLike(event, event.actionIndex)) syncPenButton(event)
            }
            MotionEvent.ACTION_BUTTON_RELEASE -> {
                if (isPenLike(event, event.actionIndex)) syncPenButton(event)
            }

            MotionEvent.ACTION_UP,
            MotionEvent.ACTION_POINTER_UP -> {
                if (isPenLike(event, actionIndex)) {
                    notePenActivity()
                    sendPenEvent(event, actionIndex, 1, vw, vh)
                } else {
                    if (stylusMode) return true
                    if (touchpadMode) {
                        val id = event.getPointerId(actionIndex)
                        if (outsideIds.remove(id)) return true
                        if (nativeActive()) feedRaw(event, padBounds, actionIndex, -1, lifted = id)
                        else feedPad(event, padBounds, actionIndex)
                        return true
                    }
                    // Not filtered on palm: whatever the contact is called
                    // now, if we sent its DOWN we owe the host its UP.
                    val slot = slotOf(event, actionIndex)
                    if (downSlots.remove(slot)) {
                        val (nx, ny) = mapCoordinates(event.getX(actionIndex), event.getY(actionIndex), vw, vh)
                        sendTouch(nx, ny, 0.0, 1, slot)
                    }
                }
            }

            MotionEvent.ACTION_CANCEL -> {
                if (stylusMode) return true
                if (touchpadMode) {
                    outsideIds.clear()
                    pad.cancel()
                    liftRaw()
                    return true
                }
                for (i in 0 until pointerCount) {
                    if (isPalm(event, i)) continue
                    val (nx, ny) = mapCoordinates(event.getX(i), event.getY(i), vw, vh)
                    sendTouch(nx, ny, 0.0, 1, slotOf(event, i))
                }
            }
        }
        return true
    }

    /**
     * Multitouch slot for a pointer, derived from its stable pointer *id*.
     *
     * The pointer *index* must not be used here: Android repacks indices
     * whenever a finger lifts, so with two fingers down, lifting the first
     * renumbers the second from index 1 to index 0. The host would then see
     * slot 1 released while slot 0 keeps moving under a different finger, and
     * pinch and two-finger scroll come apart. The pointer id stays with the
     * finger for the whole gesture.
     *
     * Clamped to the 10 slots the uinput touchscreen declares.
     */
    private fun slotOf(event: MotionEvent, index: Int): Int {
        return try {
            event.getPointerId(index).coerceIn(0, 9)
        } catch (_: Exception) {
            index.coerceIn(0, 9)
        }
    }

    /** Stylus or its eraser end — both drive the pen/tablet device. */
    private fun isPenLike(event: MotionEvent, index: Int): Boolean {
        return try {
            val t = event.getToolType(index)
            t == MotionEvent.TOOL_TYPE_STYLUS || t == MotionEvent.TOOL_TYPE_ERASER
        } catch (_: Exception) {
            false
        }
    }

    private fun isEraser(event: MotionEvent, index: Int): Boolean {
        return try {
            event.getToolType(index) == MotionEvent.TOOL_TYPE_ERASER
        } catch (_: Exception) {
            false
        }
    }

    private fun getAxis(event: MotionEvent, axis: Int, index: Int): Double {
        return try {
            event.getAxisValue(axis, index).toDouble()
        } catch (_: Exception) {
            0.0
        }
    }

    private fun getHistoricalAxis(event: MotionEvent, axis: Int, index: Int, hist: Int): Double {
        return try {
            event.getHistoricalAxisValue(axis, index, hist).toDouble()
        } catch (_: Exception) {
            0.0
        }
    }

    /**
     * Decompose Android's stylus tilt into X/Y tilt angles, **in degrees**.
     *
     * Android exposes AXIS_TILT as the angle from the screen normal (0 =
     * perpendicular, π/2 = flat) and AXIS_ORIENTATION as the azimuth of the
     * tilt around that normal (0..2π). A Wacom-style ABS_TILT_X/Y device wants
     * the signed X and Y tilt *angles*, which are the arctangents of the tilt
     * vector's components projected onto the surface — not the components
     * themselves.
     *
     * The previous version returned the raw projections `sin(tilt)·cos(orient)`
     * (a dimensionless value in [-1,1]) and the host multiplied them by
     * 180/π as if they were radians. A pen laid flat at 90° came out as 57°,
     * and everything in between was wrong non-linearly.
     */
    private fun decomposeTilt(tiltRad: Double, orientationRad: Double): Pair<Double, Double> {
        val sinTilt = sin(tiltRad)
        val cosTilt = cos(tiltRad)
        // Android's AXIS_ORIENTATION is measured clockwise from the top of the
        // screen: 0 points up, +pi/2 points right. So the direction the pen
        // leans, in screen coordinates with y downwards, is
        // (sin(orientation), -cos(orientation)).
        //
        // libinput wants tilt_x positive towards the right edge and tilt_y
        // positive towards the user, which is the bottom edge — hence the
        // minus on the y component. These two were the other way round until
        // 1.2.2, so a pen leaning right reported as leaning down; reported by
        // a Lenovo Tab Pen Plus user in issue #11.
        val tx = atan2(sinTilt * sin(orientationRad), cosTilt)
        val ty = atan2(-sinTilt * cos(orientationRad), cosTilt)
        return Math.toDegrees(tx) to Math.toDegrees(ty)
    }

    private fun sendPenEvent(event: MotionEvent, index: Int, action: Int,
                              vw: Float, vh: Float) {
        val (x, y) = mapCoordinates(event.getX(index), event.getY(index), vw, vh)
        val pressure = event.getPressure(index).toDouble()
        val (tiltX, tiltY) = decomposeTilt(
            getAxis(event, MotionEvent.AXIS_TILT, index),
            getAxis(event, MotionEvent.AXIS_ORIENTATION, index))
        emitPen(x.toDouble(), y.toDouble(), pressure, tiltX, tiltY,
            isEraser(event, index), action)
        syncPenButton(event)
    }

    /** The pen's side buttons as the host last heard them. */
    private var penButtonDown = false
    private var penSecondaryDown = false

    /**
     * Send the side buttons whenever their state differs from what the host knows, read from the
     * event's own button state: the level, not the press event (S-Pen-Mouse does the same).
     * Android reports a press as a separate ACTION_BUTTON_PRESS only some of the time, and never
     * through the touch listener while the pen hovers, which is when the button is used to pan or
     * right-click (#23, #25). BUTTON_STYLUS_PRIMARY is the S Pen's one button; some Samsung builds
     * report it as BUTTON_SECONDARY instead, which counts as the same button.
     */
    private fun syncPenButton(event: MotionEvent) {
        val state = event.buttonState
        val primary = (state and (MotionEvent.BUTTON_STYLUS_PRIMARY or MotionEvent.BUTTON_SECONDARY)) != 0
        val secondary = (state and MotionEvent.BUTTON_STYLUS_SECONDARY) != 0
        if (primary != penButtonDown) {
            penButtonDown = primary
            sendPenButton(if (primary) 5 else 6)
        }
        if (secondary != penSecondaryDown) {
            penSecondaryDown = secondary
            sendPenButton(if (secondary) 7 else 8)
        }
    }

    private fun emitPen(x: Double, y: Double, pressure: Double,
                        tiltX: Double, tiltY: Double, eraser: Boolean, action: Int) {
        val msg = JSONObject().apply {
            put("type", "pen")
            put("x", x)
            put("y", y)
            put("pressure", pressure.coerceIn(0.0, 1.0))
            put("tilt_x", tiltX)
            put("tilt_y", tiltY)
            put("eraser", eraser)
            put("action", action)
            put("display_id", targetDisplayId)
        }
        webSocket?.send(msg.toString())
    }

    /** [action]: 5 / 6 first button down / up, 7 / 8 second button down / up. */
    private fun sendPenButton(action: Int) {
        val msg = JSONObject().apply {
            put("type", "pen")
            put("x", 0.0)
            put("y", 0.0)
            put("pressure", 0.0)
            put("tilt_x", 0.0)
            put("tilt_y", 0.0)
            put("eraser", false)
            put("action", action)
        }
        webSocket?.send(msg.toString())
    }

    private fun sendPenProximityExit() {
        // Android sends a hover exit just before the tip touches down, and the side button can be
        // held through it. Letting go of the button at once would send a release and a new press
        // around every touch-down (a spurious right-button release, which opens a context menu).
        // So the buttons are only released if the pen really stays away for a moment.
        mainHandler.removeCallbacks(releasePenButtons)
        mainHandler.postDelayed(releasePenButtons, PEN_BUTTON_GRACE_MS)
        val msg = JSONObject().apply {
            put("type", "pen")
            put("x", 0.0)
            put("y", 0.0)
            put("pressure", 0.0)
            put("tilt_x", 0.0)
            put("tilt_y", 0.0)
            put("eraser", false)
            put("action", 4) // HOVER_EXIT / pen left proximity
        }
        webSocket?.send(msg.toString())
    }

    private val releasePenButtons: Runnable = Runnable {
        if (penButtonDown) {
            penButtonDown = false
            sendPenButton(6)
        }
        if (penSecondaryDown) {
            penSecondaryDown = false
            sendPenButton(8)
        }
    }

    private fun sendTouch(x: Float, y: Float, pressure: Double,
                          action: Int, slot: Int) {
        val msg = JSONObject().apply {
            put("type", "touch")
            put("x", x.toDouble())
            put("y", y.toDouble())
            put("pressure", pressure.coerceIn(0.0, 1.0))
            put("action", action)
            put("slot", slot)
            put("display_id", targetDisplayId)
        }
        webSocket?.send(msg.toString())
    }

    /**
     * Push encoder settings to the host. The host live-restarts ffmpeg with
     * the new parameters and persists them in its config file. An offline or rejected change
     * waits for a connection; one the socket accepted is not replayed.
     */
    @Synchronized
    fun sendConfig(bitrateKbps: Int, fps: Int) {
        val msg = JSONObject().apply {
            put("type", "config")
            put("bitrate", bitrateKbps)
            put("fps", fps)
        }
        pendingConfig = msg
        if (isConnected) {
            webSocket?.let { sendPendingConfig(it) }
        }
    }

    /** Send the waiting config, if any, and forget it once the socket has taken it. */
    @Synchronized
    private fun sendPendingConfig(socket: WebSocket) {
        val msg = pendingConfig ?: return
        if (socket.send(msg.toString())) {
            pendingConfig = null
            Log.i(TAG, "Sent config: $msg")
        }
    }

    /**
     * Tell the host that frame [seq] is on screen. The host started the clock
     * when it emitted that frame, so the round trip it computes is the real
     * end-to-end latency without either side needing a shared time base.
     */
    fun sendRendered(seq: Int, decodeUs: Int) {
        if (!isConnected) return
        val msg = JSONObject().apply {
            put("type", "rendered")
            // Sent unsigned: the host's counter is a u32 and Kotlin's Int is
            // signed, so it wraps negative after ~2^31 frames (~1 year at
            // 60 fps, but free to get right).
            put("seq", seq.toLong() and 0xFFFFFFFFL)
            // How much of the round trip was spent here (arrival → on screen).
            // The host subtracts it to see what the wire actually costs.
            if (decodeUs >= 0) put("decode_us", decodeUs)
        }
        webSocket?.send(msg.toString())
    }

    /**
     * Ask the host to switch between being a second screen and being a
     * graphics tablet. The host applies it and answers with its new mode, so
     * the UI follows [onModeKnown] rather than assuming this succeeded.
     */
    fun sendMode(penOnly: Boolean) {
        val msg = JSONObject().apply {
            put("type", "mode")
            put("pen_only", penOnly)
        }
        if (isConnected) {
            webSocket?.send(msg.toString())
            Log.i(TAG, "Requested mode: ${if (penOnly) "pen-only" else "display"}")
        } else {
            // Held rather than replayed forever: the host is the source of
            // truth for the mode, and re-asserting a stale choice on every
            // reconnect would fight whatever it was set to in the meantime.
            pendingMode = msg
        }
    }

    /**
     * Tell the host a setting was changed on this tablet (`touchpad_mode`, `immersive_mode`,
     * `battery_saver_on_disconnect`). The host stores it as this device's override and answers with a
     * `config_update`, which is what the UI follows. Dropped while offline: the host is the source of
     * truth, so a stale choice must not overwrite what was set on the computer in the meantime.
     */
    fun sendSetting(key: String, value: Any) {
        if (!isConnected) return
        webSocket?.send(JSONObject().apply {
            put("type", "setting")
            put("key", key)
            put("value", value)
        }.toString())
    }

    private var lastWakeMs = 0L

    /**
     * Tell the host the user touched the tablet (the app swallowed a touch to wake a dimmed screen, or a
     * finger came down on the touchpad). It counts as use for the idle timer and, in touchpad mode, makes
     * the host nudge its pointer by one pixel so an auto-hidden cursor reappears. Throttled to one a second.
     */
    fun sendWake(force: Boolean = true) {
        val now = android.os.SystemClock.elapsedRealtime()
        if (!force && now - lastWakeMs < 1000) return
        lastWakeMs = now
        if (isConnected) webSocket?.send("""{"type":"wake"}""")
    }

    /**
     * Send a foreground hello/reconnect ping to the host so it re-asserts
     * adb reverse port forwarding immediately upon app resume.
     */
    fun sendPing() {
        val msg = JSONObject().apply {
            put("type", "ping")
        }
        if (isConnected) {
            webSocket?.send(msg.toString())
            Log.i(TAG, "Sent reconnect ping to host")
        } else {
            connect()
        }
    }

    /**
     * Tell the host which transport (USB or Wi-Fi) the user prefers; the host stores it in this device's profile
and consults it when choosing between the device's adb entries.
     */
    fun sendPreferredTransport(transport: String) {
        currentPreferredTransport = transport
        val msg = JSONObject().apply {
            put("type", "preferred_transport")
            put("transport", transport)
        }
        if (isConnected) {
            webSocket?.send(msg.toString())
            Log.i(TAG, "Sent preferred transport to host: $transport")
        }
    }

    /**
     * MotionEvent.TOOL_TYPE_PALM exists from API 29. The value is stable
     * (6) and older devices simply never report it, so comparing against the
     * number is correct everywhere; the annotation only tells lint that the
     * comparison is deliberate.
     */
    @android.annotation.SuppressLint("WrongConstant")
    /** Slots whose DOWN went to the host and whose UP therefore must too. */
    private val downSlots = HashSet<Int>()

    /**
     * When the pen was last seen, hovering or drawing.
     *
     * Filtering on TOOL_TYPE_PALM alone is not enough: a hand resting on the
     * glass while drawing arrives as an ordinary finger on this tablet, and
     * the desktop happily scrolled and clicked with it. While the pen is in
     * play the fingers are the hand holding it, so they are dropped — which
     * is what every drawing tablet does.
     */
    @Volatile private var lastPenNanos = 0L
    private val PEN_OWNS_SCREEN_NS = 500_000_000L

    private fun penIsActive(): Boolean =
        lastPenNanos != 0L && System.nanoTime() - lastPenNanos < PEN_OWNS_SCREEN_NS

    /** The pen took over: anything a finger left pressed has to be lifted. */
    private fun notePenActivity() {
        mainHandler.removeCallbacks(releasePenButtons)
        lastPenNanos = System.nanoTime()
        liftTouches()
    }

    /**
     * Touchpad mode: fingers move the computer's pointer like a laptop touchpad instead of touching
     * the screen where they land. The pen is not affected. Recognition happens here; the host only
     * receives plain `rel`, `button` and `scroll` messages.
     */
    @Volatile var touchpadMode = false
        private set

    private val mainHandler = Handler(Looper.getMainLooper())
    private val padTimer: Runnable = Runnable { pad.onTimer(SystemClock.uptimeMillis() / 1000.0) }

    /** Millimetres per tablet pixel, from the panel's real size when the host told us, else a typical tablet. */
    private fun mmPerPx(): Float {
        val longPx = max(nativeWidth, nativeHeight)
        val longMm = max(nativeWidthMm, nativeHeightMm)
        return if (longPx > 0 && longMm > 0) longMm.toFloat() / longPx else DEFAULT_MM_PER_PX
    }

    private val pad: TouchpadEngine = TouchpadEngine(object : TouchpadEngine.Output {
        // The engine speaks millimetres; the host wants tablet pixels, as it always has.
        override fun move(dxMm: Float, dyMm: Float) {
            val k = PAD_GAIN / mmPerPx()
            sendRel("rel", dxMm * k, dyMm * k)
        }
        override fun scroll(dxMm: Float, dyMm: Float) {
            val k = 1f / mmPerPx()
            sendRel("scroll", dxMm * k, dyMm * k)
        }
        override fun pinch(deltaMm: Float) = sendGesture("pinch", (deltaMm / mmPerPx()).toDouble())
        override fun pinchEnd() = sendGesture("pinch_end", 0.0)
        override fun swipe(fingers: Int, direction: TouchpadEngine.Swipe) {
            sendGesture(
                when (direction) {
                    TouchpadEngine.Swipe.LEFT -> "workspace_next"
                    TouchpadEngine.Swipe.RIGHT -> "workspace_prev"
                    TouchpadEngine.Swipe.UP, TouchpadEngine.Swipe.DOWN -> "overview"
                },
                0.0,
            )
        }
        override fun button(button: Int, down: Boolean) {
            webSocket?.send(JSONObject().apply {
                put("type", "button")
                put("button", button)
                put("down", down)
            }.toString())
            if (down) onHaptic?.invoke()
        }
    }, schedule = { delayMs ->
        mainHandler.removeCallbacks(padTimer)
        mainHandler.postDelayed(padTimer, delayMs)
    }).also { it.log = { line -> if (DEBUG_INPUT) Log.d("UScreenInput", line) } }

    private fun sendGesture(kind: String, amount: Double) {
        webSocket?.send(JSONObject().apply {
            put("type", "gesture")
            put("kind", kind)
            put("amount", amount)
        }.toString())
    }

    /**
     * Turn the fingers of [event] into engine frames: one for each sample Android batched into a
     * move, one for anything else. [excludeIndex] is a pointer that has just lifted. Contacts outside
     * the touch area are ignored; one that slides out stops at the edge, like on a real pad.
     */
    private fun feedPad(event: MotionEvent, bounds: TouchpadShape.Bounds, excludeIndex: Int) {
        val k = mmPerPx()
        pad.padWidthMm = bounds.width * k
        pad.padHeightMm = bounds.height * k
        pad.rightClickZone = hostConfig.rightClickMethod != HostConfig.RIGHT_CLICK_TWO_FINGER
        val history = if (event.actionMasked == MotionEvent.ACTION_MOVE) event.historySize else 0
        for (h in 0..history) {
            val sample = if (h < history) h else -1
            val fingers = ArrayList<FingerState>(event.pointerCount)
            for (i in 0 until event.pointerCount) {
                if (i == excludeIndex || isPenLike(event, i)) continue
                val id = event.getPointerId(i)
                if (id in outsideIds) continue
                val x = if (sample >= 0) event.getHistoricalX(i, sample) else event.getX(i)
                val y = if (sample >= 0) event.getHistoricalY(i, sample) else event.getY(i)
                val major = if (sample >= 0) event.getHistoricalTouchMajor(i, sample) else event.getTouchMajor(i)
                val minor = if (sample >= 0) event.getHistoricalTouchMinor(i, sample) else event.getTouchMinor(i)
                val pressure = if (sample >= 0) event.getHistoricalPressure(i, sample) else event.getPressure(i)
                fingers.add(
                    FingerState(
                        id = id,
                        x = (bounds.clampX(x) - bounds.left) * k,
                        y = (bounds.clampY(y) - bounds.top) * k,
                        touchMajor = major * k,
                        touchMinor = minor * k,
                        pressure = pressure,
                        toolPalm = isPalm(event, i),
                    ),
                )
            }
            val t = if (sample >= 0) event.getHistoricalEventTime(sample) else event.eventTime
            pad.onFrame(HardwareState(t / 1000.0, fingers))
        }
    }


    // ---- Native gestures: raw fingers for the computer's libinput -------------------------------
    //
    // With native gestures the tablet does not recognise anything. Every finger that is down is sent
    // as it is (position in millimetres from the corner of the touch area, pressure) and the host
    // replays them on a virtual multi-touch touchpad; libinput and GNOME / KDE do the motion, taps,
    // scrolling, pinch and 3 / 4 finger swipes. Only the touch area (shape) and the bottom-right
    // click strip are decided here, on the raw coordinates, before anything is sent.

    /** True when Touchpad mode should send raw fingers instead of running the gesture engine. */
    private fun nativeActive(): Boolean = touchpadMode && hostConfig.nativeGestures

    /** Whether the last raw frame had fingers in it, so a lift is only sent when one is owed. */
    private var rawDown = false

    /** The first finger of a touch when it landed in the click strip: held back, a tap there is a right click. */
    private data class ZoneFinger(val id: Int, val downMs: Long, val x: Float, val y: Float, var moved: Boolean)
    private var zoneFinger: ZoneFinger? = null

    /**
     * Send the fingers of [event] as raw frames: one for each sample Android batched into a move, one
     * for anything else. [excludeIndex] is a pointer that has just lifted ([lifted] is its id);
     * [downIndex] one that has just landed.
     */
    private fun feedRaw(event: MotionEvent, bounds: TouchpadShape.Bounds, excludeIndex: Int, downIndex: Int, lifted: Int = -1) {
        val k = mmPerPx()
        val now = event.eventTime
        val zoneOn = hostConfig.rightClickMethod == HostConfig.RIGHT_CLICK_ZONE
        // A finger landing alone in the bottom-right strip is a possible right click and is not sent.
        if (downIndex >= 0 && zoneOn && zoneFinger == null && rawDown.not()) {
            val x = event.getX(downIndex)
            val y = event.getY(downIndex)
            val inZone = x >= bounds.left + bounds.width / 2f && y >= bounds.top + bounds.height * (1f - TouchpadShape.ZONE_FRACTION)
            if (inZone && countPadFingers(event, excludeIndex) == 1) {
                zoneFinger = ZoneFinger(event.getPointerId(downIndex), now, x, y, false)
            }
        } else if (downIndex >= 0 && zoneFinger != null && event.getPointerId(downIndex) != zoneFinger?.id) {
            // A second finger: the held one is part of a multi-finger gesture after all, so it joins it.
            zoneFinger = null
        }
        zoneFinger?.let { z ->
            for (i in 0 until event.pointerCount) {
                if (event.getPointerId(i) != z.id || i == excludeIndex) continue
                val dx = (event.getX(i) - z.x) * k
                val dy = (event.getY(i) - z.y) * k
                if (dx * dx + dy * dy > ZONE_TAP_SLOP_MM * ZONE_TAP_SLOP_MM) z.moved = true
            }
        }
        if (lifted >= 0) {
            zoneFinger?.let { z ->
                if (z.id == lifted) {
                    if (!z.moved && now - z.downMs <= ZONE_TAP_MS) sendRightClick()
                    zoneFinger = null
                }
            }
        }

        val sens = hostConfig.touchpadSensitivity.coerceIn(50, 200) / 100f
        val history = if (event.actionMasked == MotionEvent.ACTION_MOVE) event.historySize else 0
        for (h in 0..history) {
            val sample = if (h < history) h else -1
            val slots = org.json.JSONArray()
            for (i in 0 until event.pointerCount) {
                if (i == excludeIndex || isPenLike(event, i) || isPalm(event, i)) continue
                val id = event.getPointerId(i)
                if (id in outsideIds || id == zoneFinger?.id) continue
                val x = if (sample >= 0) event.getHistoricalX(i, sample) else event.getX(i)
                val y = if (sample >= 0) event.getHistoricalY(i, sample) else event.getY(i)
                val p = if (sample >= 0) event.getHistoricalPressure(i, sample) else event.getPressure(i)
                // The touch area sits in the middle of the host's pad, scaled by the sensitivity, so
                // libinput's edge and button zones along the pad's border are never reached.
                slots.put(JSONObject().apply {
                    put("id", id)
                    put("x", (NATIVE_PAD_W_MM / 2f + ((bounds.clampX(x) - bounds.left) - bounds.width / 2f) * k * sens).toDouble())
                    put("y", (NATIVE_PAD_H_MM / 2f + ((bounds.clampY(y) - bounds.top) - bounds.height / 2f) * k * sens).toDouble())
                    put("pressure", p.toDouble())
                })
            }
            sendRaw(slots)
        }
    }

    private fun countPadFingers(event: MotionEvent, excludeIndex: Int): Int {
        var n = 0
        for (i in 0 until event.pointerCount) {
            if (i == excludeIndex || isPenLike(event, i) || isPalm(event, i)) continue
            if (event.getPointerId(i) in outsideIds) continue
            n++
        }
        return n
    }

    private fun sendRaw(slots: org.json.JSONArray) {
        val empty = slots.length() == 0
        if (empty && !rawDown) return
        rawDown = !empty
        webSocket?.send(JSONObject().apply {
            put("type", "touchpad_raw")
            put("slots", slots)
        }.toString())
    }

    /** Right click from the click strip: the host's relative mouse, since the pad only sees fingers. */
    private fun sendRightClick() {
        for (down in listOf(true, false)) {
            webSocket?.send(JSONObject().apply {
                put("type", "button")
                put("button", 1)
                put("down", down)
            }.toString())
        }
        onHaptic?.invoke()
    }

    /** Lift every finger the host's touchpad still holds, and forget a click-strip finger. */
    private fun liftRaw() {
        zoneFinger = null
        if (rawDown) sendRaw(org.json.JSONArray())
    }

    /**
     * The native-gestures setting (the host's effective value) may have changed: whatever the old
     * mode was holding is let go, so a finger cannot be left on the host's pad or in the engine.
     */
    fun refreshNativeGestures() {
        val now = nativeActive()
        if (now == lastNative) return
        lastNative = now
        pad.cancel()
        liftRaw()
    }
    private var lastNative = false

    /** Pointer ids whose finger landed outside the touch area; they are ignored until they lift. */
    private val outsideIds = HashSet<Int>()

    /** Which part of the screen is the touchpad (`full_screen`, `standard_rectangle`, `wide_rectangle`). */
    @Volatile var touchpadShape = TouchpadShape.FULL
        private set

    fun setTouchpadShape(shape: String) {
        if (touchpadShape == shape) return
        touchpadShape = shape
        // Fingers that were down belong to the old area.
        outsideIds.clear()
        pad.cancel()
    }

    /**
     * Stylus mode: only the pen and its eraser are sent, as absolute coordinates; every finger is
     * ignored. Switching drops whatever a finger still held.
     */
    @Volatile var stylusMode = false
        private set

    fun setStylusMode(on: Boolean) {
        if (stylusMode == on) return
        stylusMode = on
        liftTouches()
    }

    /** Switch between direct touch and touchpad behaviour, letting go of anything the old mode held. */
    fun setTouchpadMode(on: Boolean) {
        if (touchpadMode == on) return
        touchpadMode = on
        liftTouches()
    }

    private fun sendRel(type: String, dx: Float, dy: Float) {
        webSocket?.send(JSONObject().apply {
            put("type", type)
            put("dx", dx.toDouble())
            put("dy", dy.toDouble())
        }.toString())
    }

    /** Release every finger the host still thinks is pressed (and any button the touchpad holds). */
    private fun liftTouches() {
        pad.cancel()
        liftRaw()
        if (downSlots.isNotEmpty()) {
            for (slot in downSlots.toList()) sendTouch(0f, 0f, 0.0, 1, slot)
            downSlots.clear()
        }
    }

    /**
     * True while the locked-device overlay is up and the user has not allowed touch on the lock screen:
     * nothing (finger or pen) is forwarded, and whatever was pressed is lifted so it cannot stay stuck.
     */
    @Volatile var inputBlocked = false
        private set

    /**
     * Block or unblock all input (used while the locked-device overlay is shown); blocking lifts anything pressed.
     */
    fun setInputBlocked(blocked: Boolean) {
        inputBlocked = blocked
        if (blocked) liftTouches()
    }

    /**
     * Fingers are only sent while this is on; the pen always is. Turning it
     * off lifts whatever was pressed so nothing stays stuck on the host.
     */
    @Volatile var touchEnabled = true
        private set

    /**
     * Enable or disable finger input (the pen always works); disabling lifts anything pressed.
     */
    fun setTouchEnabled(on: Boolean) {
        touchEnabled = on
        if (!on) liftTouches()
    }

    // Lint only knows the public tool types; PALM is hidden but real.
    @Suppress("WrongConstant")
    private fun isPalm(event: MotionEvent, index: Int): Boolean =
        event.getToolType(index) == TOOL_TYPE_PALM

    /**
     * True while the input WebSocket is open.
     */
    fun isControlConnected(): Boolean = isConnected

    /**
     * Close the socket and cancel the reconnect loop.
     */
    fun disconnect() {
        reconnectJob?.cancel()
        webSocket?.close(1000, "Client closing")
        webSocket = null
        isConnected = false
    }
}

/** A state the host announced over the input socket; the UI covers the screen with an overlay while it holds. */
data class HostNotice(val kind: Kind, val host: String = "", val device: String = "") {
    enum class Kind { PAUSED, SETTINGS, UPDATING }
}
