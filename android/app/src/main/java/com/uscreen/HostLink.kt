package com.uscreen

import android.view.animation.AccelerateDecelerateInterpolator
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.view.animation.Interpolator
import android.view.animation.LinearInterpolator
import android.view.animation.PathInterpolator
import org.json.JSONObject
import kotlin.math.pow

/*
 * Pure pieces of the host link: what the host pushes as settings, how long a lost link is retried
 * before the app calls the host offline, and the fade curves of the idle dimming. None of it needs
 * a device, so all of it is covered by JVM tests.
 */

/**
 * The per-device settings the host pushes as `config_update` (global default merged with this
 * device's override) and that the app mirrors into [Prefs] and its UI. The tablet's own toggles go
 * the other way as `setting` messages, so both sides always show the same values.
 *
 * [inputMode] is exactly one of `display`, `touchpad`, `stylus`. [wifiConfigured] is informational
 * (the host knows a Wi-Fi address for this tablet, so the pairing button is hidden).
 */
data class HostConfig(
    val inputMode: String = MODE_DISPLAY,
    val immersiveMode: Boolean = false,
    val showStats: Boolean = false,
    val touchInput: Boolean = true,
    val touchOnLockScreen: Boolean = false,
    val keepDisplayAliveWhenLocked: Boolean = true,
    val keepConnectedInBackground: Boolean = false,
    val blankPenOnly: Boolean = true,
    val orientation: String = "auto",
    val themeMode: String = "system",
    val batterySaverOnDisconnect: Boolean = false,
    val batterySaverLock: Boolean = false,
    val idleDimEnabled: Boolean = false,
    val idleDimTimeoutSecs: Int = 60,
    val dimCurve: String = "ease_out",
    val undimCurve: String = "ease_in",
    val dimDurationMs: Long = 3000,
    val undimDurationMs: Long = 500,
    val wifiConfigured: Boolean = false,
    val touchpadHaptics: Boolean = true,
    /** Touchpad mode sends raw fingers for the computer's libinput to recognise (the host's effective value). */
    val nativeGestures: Boolean = true,
    val rightClickMethod: String = RIGHT_CLICK_ZONE,
    val touchpadShape: String = TouchpadShape.FULL,
    val stylusStyle: String = STYLUS_MOUSE,
    val stylusButtonPrimary: String = PEN_BUTTON_RIGHT,
    val stylusButtonSecondary: String = PEN_BUTTON_MIDDLE,
) {
    /** True while the host streams video to this tablet (only Display mode does). */
    val streamsVideo: Boolean get() = inputMode == MODE_DISPLAY

    /** Copy with one setting changed, as the tablet's own toggles do; null for an unknown key or a wrong type. */
    fun withSetting(key: String, value: Any): HostConfig? = when (key) {
        "input_mode" -> (value as? String)?.takeIf { it in MODES }?.let { copy(inputMode = it) }
        "immersive_mode" -> (value as? Boolean)?.let { copy(immersiveMode = it) }
        "show_stats" -> (value as? Boolean)?.let { copy(showStats = it) }
        "touch_input" -> (value as? Boolean)?.let { copy(touchInput = it) }
        "touch_on_lock_screen" -> (value as? Boolean)?.let { copy(touchOnLockScreen = it) }
        "keep_display_alive_when_locked" -> (value as? Boolean)?.let { copy(keepDisplayAliveWhenLocked = it) }
        "keep_connected_in_background" -> (value as? Boolean)?.let { copy(keepConnectedInBackground = it) }
        "blank_pen_only" -> (value as? Boolean)?.let { copy(blankPenOnly = it) }
        "orientation" -> (value as? String)?.let { copy(orientation = it) }
        "theme_mode" -> (value as? String)?.let { copy(themeMode = it) }
        "battery_saver_on_disconnect" -> (value as? Boolean)?.let { copy(batterySaverOnDisconnect = it) }
        "battery_saver_lock" -> (value as? Boolean)?.let { copy(batterySaverLock = it) }
        "idle_dim_enabled" -> (value as? Boolean)?.let { copy(idleDimEnabled = it) }
        "idle_dim_timeout_secs" -> (value as? Int)?.let { copy(idleDimTimeoutSecs = it) }
        "touchpad_shape" -> (value as? String)?.takeIf { it in TouchpadShape.SHAPES }?.let { copy(touchpadShape = it) }
        "touchpad_haptics" -> (value as? Boolean)?.let { copy(touchpadHaptics = it) }
        "native_gestures" -> (value as? Boolean)?.let { copy(nativeGestures = it) }
        "right_click_method" -> (value as? String)?.takeIf { it in RIGHT_CLICK_METHODS }?.let { copy(rightClickMethod = it) }
        "stylus_style" -> (value as? String)?.takeIf { it in STYLUS_STYLES }?.let { copy(stylusStyle = it) }
        "stylus_button_primary" -> (value as? String)?.takeIf { it in PEN_BUTTONS }?.let { copy(stylusButtonPrimary = it) }
        "stylus_button_secondary" -> (value as? String)?.takeIf { it in PEN_BUTTONS }?.let { copy(stylusButtonSecondary = it) }
        else -> null
    }

    companion object {
        const val MODE_DISPLAY = "display"
        const val MODE_TOUCHPAD = "touchpad"
        const val MODE_STYLUS = "stylus"
        val MODES = listOf(MODE_DISPLAY, MODE_TOUCHPAD, MODE_STYLUS)

        /** A tap in the bottom-right strip right-clicks (default). */
        const val RIGHT_CLICK_ZONE = "bottom_right_zone"

        /** A two-finger tap right-clicks. */
        const val RIGHT_CLICK_TWO_FINGER = "two_finger_tap"
        val RIGHT_CLICK_METHODS = listOf(RIGHT_CLICK_ZONE, RIGHT_CLICK_TWO_FINGER)

        /** Stylus mode: the pen is a mouse pointer (default), or a pressure and tilt graphics tablet. */
        const val STYLUS_MOUSE = "mouse"
        const val STYLUS_TABLET = "tablet"
        val STYLUS_STYLES = listOf(STYLUS_MOUSE, STYLUS_TABLET)

        /** What a pen side button does in mouse style. */
        const val PEN_BUTTON_RIGHT = "right"
        const val PEN_BUTTON_MIDDLE = "middle"
        const val PEN_BUTTON_NONE = "none"
        val PEN_BUTTONS = listOf(PEN_BUTTON_RIGHT, PEN_BUTTON_MIDDLE, PEN_BUTTON_NONE)

        /** Read a `config_update` message; fields it does not carry keep [base]'s value. */
        fun fromJson(o: JSONObject, base: HostConfig): HostConfig = HostConfig(
            inputMode = o.optString("input_mode", base.inputMode).takeIf { it in MODES } ?: base.inputMode,
            immersiveMode = o.optBoolean("immersive_mode", base.immersiveMode),
            showStats = o.optBoolean("show_stats", base.showStats),
            touchInput = o.optBoolean("touch_input", base.touchInput),
            touchOnLockScreen = o.optBoolean("touch_on_lock_screen", base.touchOnLockScreen),
            keepDisplayAliveWhenLocked = o.optBoolean("keep_display_alive_when_locked", base.keepDisplayAliveWhenLocked),
            keepConnectedInBackground = o.optBoolean("keep_connected_in_background", base.keepConnectedInBackground),
            blankPenOnly = o.optBoolean("blank_pen_only", base.blankPenOnly),
            orientation = o.optString("orientation", base.orientation),
            themeMode = o.optString("theme_mode", base.themeMode),
            batterySaverOnDisconnect = o.optBoolean("battery_saver_on_disconnect", base.batterySaverOnDisconnect),
            batterySaverLock = o.optBoolean("battery_saver_lock", base.batterySaverLock),
            idleDimEnabled = o.optBoolean("idle_dim_enabled", base.idleDimEnabled),
            idleDimTimeoutSecs = o.optInt("idle_dim_timeout_secs", base.idleDimTimeoutSecs),
            dimCurve = o.optString("dim_curve", base.dimCurve),
            undimCurve = o.optString("undim_curve", base.undimCurve),
            dimDurationMs = o.optLong("dim_duration_ms", base.dimDurationMs),
            undimDurationMs = o.optLong("undim_duration_ms", base.undimDurationMs),
            wifiConfigured = o.optBoolean("wifi_configured", base.wifiConfigured),
            touchpadHaptics = o.optBoolean("touchpad_haptics", base.touchpadHaptics),
            nativeGestures = o.optBoolean("native_gestures", base.nativeGestures),
            rightClickMethod = o.optString("right_click_method", base.rightClickMethod)
                .takeIf { it in RIGHT_CLICK_METHODS } ?: base.rightClickMethod,
            touchpadShape = o.optString("touchpad_shape", base.touchpadShape)
                .takeIf { it in TouchpadShape.SHAPES } ?: base.touchpadShape,
            stylusStyle = o.optString("stylus_style", base.stylusStyle)
                .takeIf { it in STYLUS_STYLES } ?: base.stylusStyle,
            stylusButtonPrimary = o.optString("stylus_button_primary", base.stylusButtonPrimary)
                .takeIf { it in PEN_BUTTONS } ?: base.stylusButtonPrimary,
            stylusButtonSecondary = o.optString("stylus_button_secondary", base.stylusButtonSecondary)
                .takeIf { it in PEN_BUTTONS } ?: base.stylusButtonSecondary,
        )

        /** The saved copy of the synced settings, so a restart shows the last known values before the host answers. */
        fun fromPrefs(prefs: Prefs): HostConfig = HostConfig(
            inputMode = prefs.inputMode,
            immersiveMode = prefs.immersiveModeEnabled,
            showStats = prefs.showStats,
            touchInput = prefs.touchEnabled,
            touchOnLockScreen = prefs.touchOnLockScreen,
            keepDisplayAliveWhenLocked = prefs.keepDisplayAliveWhenLocked,
            keepConnectedInBackground = prefs.keepConnectedInBackground,
            blankPenOnly = prefs.blankPenOnly,
            orientation = Prefs.orientationName(prefs.orientation),
            themeMode = Prefs.themeName(prefs.themeMode),
            batterySaverOnDisconnect = prefs.batterySaverOnDisconnect,
            batterySaverLock = prefs.batterySaverLock,
            wifiConfigured = prefs.wifiConfigured,
            touchpadHaptics = prefs.touchpadHaptics,
            nativeGestures = prefs.nativeGestures,
            rightClickMethod = prefs.rightClickMethod,
            touchpadShape = prefs.touchpadShape,
            stylusStyle = prefs.stylusStyle,
            stylusButtonPrimary = prefs.stylusButtonPrimary,
            stylusButtonSecondary = prefs.stylusButtonSecondary,
        )
    }
}

/**
 * Decides when a lost link to the host turns into "Host Offline".
 *
 * The app retries every [RETRY_INTERVAL_MS]; only after the link has been down continuously for
 * [offlineAfterMs] does [expired] become true. One successful connection resets the clock, so a
 * flaky cable never adds up to an offline verdict. Time is passed in so tests need no real clock.
 */
class ReconnectPolicy(private val offlineAfterMs: Long = OFFLINE_AFTER_MS) {
    companion object {
        /** Gap between two connection attempts. */
        const val RETRY_INTERVAL_MS = 2_000L

        /** Consecutive failure time after which the host counts as offline. */
        const val OFFLINE_AFTER_MS = 120_000L
    }

    private var downSince = -1L

    /** The link is up (or the app is intentionally disconnected): forget earlier failures. */
    fun linkUp() {
        downSince = -1L
    }

    /** The link is down at [nowMs]; the first call of a streak starts the clock. */
    fun linkDown(nowMs: Long) {
        if (downSince < 0) downSince = nowMs
    }

    /** How long the current streak of failures has lasted; 0 when the link is up. */
    fun downForMs(nowMs: Long): Long = if (downSince < 0) 0 else (nowMs - downSince).coerceAtLeast(0)

    /** True once the failures have lasted [offlineAfterMs] without a single success. */
    fun expired(nowMs: Long): Boolean = downSince >= 0 && downForMs(nowMs) >= offlineAfterMs
}

/** Fade curves of the idle dimming, by the names the host sends. */
object FadeCurve {
    /** Brightness the dim fades down to (5%). */
    const val DIM_LEVEL = 0.05f

    /** Brightness the undim fades back up to. */
    const val FULL_LEVEL = 1.0f

    /** Brightness battery saver drops to once the reconnect window has expired (10%). */
    const val BATTERY_SAVER_LEVEL = 0.10f

    /** Slow start, steep finish: `(2^(10t) - 1) / 1023`, so it still starts at 0 and ends at exactly 1. */
    fun exponential(t: Float): Float {
        val x = t.coerceIn(0f, 1f)
        return ((2.0.pow(10.0 * x) - 1.0) / 1023.0).toFloat()
    }

    /** Same curve as an [Interpolator]. */
    object Exponential : Interpolator {
        override fun getInterpolation(input: Float): Float = exponential(input)
    }

    /** The names the host may send; anything else is treated as linear. */
    val NAMES = listOf("linear", "ease_in", "ease_out", "ease_in_out", "exponential")

    /**
     * Control points of a `bezier:x1,y1,x2,y2` curve (a CSS-style cubic Bezier through (0,0) and (1,1)), or
     * null when the text is not one. x is clamped to 0..1 as [PathInterpolator] requires.
     */
    fun parseBezier(name: String): FloatArray? {
        val body = name.trim().removePrefix("bezier:").takeIf { it.length != name.trim().length } ?: return null
        val n = body.split(",").map { it.trim().toFloatOrNull() ?: return null }
        if (n.size != 4 || n.any { !it.isFinite() }) return null
        return floatArrayOf(n[0].coerceIn(0f, 1f), n[1], n[2].coerceIn(0f, 1f), n[3])
    }

    /** Map a curve name (a preset, or `bezier:x1,y1,x2,y2`) to its [Interpolator]. */
    fun interpolator(name: String): Interpolator {
        parseBezier(name)?.let { return PathInterpolator(it[0], it[1], it[2], it[3]) }
        return when (name.trim().lowercase()) {
            "ease_in" -> AccelerateInterpolator()
            "ease_out" -> DecelerateInterpolator()
            "ease_in_out" -> AccelerateDecelerateInterpolator()
            "exponential" -> Exponential
            else -> LinearInterpolator()
        }
    }
}
