package com.uscreen

import android.content.Context
import android.content.SharedPreferences

/** Persisted user settings on the tablet side. */
class Prefs(context: Context) {
    private val sp: SharedPreferences =
        context.getSharedPreferences("uscreen", Context.MODE_PRIVATE)

    companion object {
        /**
         * Defaults deliberately match the host's own defaults. They used to be
         * 200 Mbps / 90 fps, which the app pushed to the host on every start —
         * overwriting whatever was configured on the desktop and driving the
         * encoder far past what the USB link carries, so frames queued up and
         * latency grew without bound.
         */
        const val DEFAULT_BITRATE_KBPS = 20_000
        const val DEFAULT_FPS = 60

        /** Kept in sync with `config::MAX_BITRATE_KBPS` on the host. */
        const val MAX_BITRATE_KBPS = 60_000
        const val MIN_BITRATE_KBPS = 5_000

        /** Values of [orientation]. */
        const val ORIENTATION_AUTO = 0
        const val ORIENTATION_CAMERA_UP = 1
        const val ORIENTATION_CAMERA_DOWN = 2

        /** Values of [themeMode]; the numbers are AppCompatDelegate's own MODE_NIGHT_* constants' order, not their values. */
        const val THEME_SYSTEM = 0
        const val THEME_LIGHT = 1
        const val THEME_DARK = 2

        /** The wire names the host uses for [orientation] and [themeMode]. */
        fun orientationName(v: Int) = when (v) {
            ORIENTATION_CAMERA_UP -> "camera_up"
            ORIENTATION_CAMERA_DOWN -> "camera_down"
            else -> "auto"
        }

        fun orientationFromName(n: String) = when (n) {
            "camera_up" -> ORIENTATION_CAMERA_UP
            "camera_down" -> ORIENTATION_CAMERA_DOWN
            else -> ORIENTATION_AUTO
        }

        fun themeName(v: Int) = when (v) {
            THEME_LIGHT -> "light"
            THEME_DARK -> "dark"
            else -> "system"
        }

        fun themeFromName(n: String) = when (n) {
            "light" -> THEME_LIGHT
            "dark" -> THEME_DARK
            else -> THEME_SYSTEM
        }

        /** Values of [preferredTransport]. */
        const val TRANSPORT_AUTO = 0
        const val TRANSPORT_USB = 1
        const val TRANSPORT_WIFI = 2
    }

    /** The host sent "unpaired" (Forget Device); cleared when the host greets this app again. */
    var unpairedByHost: Boolean
        get() = sp.getBoolean("unpaired_by_host", false)
        set(v) = sp.edit().putBoolean("unpaired_by_host", v).apply()

    /** Light, Dark or System default ([THEME_SYSTEM], the default). */
    var themeMode: Int
        get() = sp.getInt("theme_mode", THEME_SYSTEM)
        set(v) = sp.edit().putInt("theme_mode", v).apply()

    /**
     * Forward touches while the device is locked. Off by default: a locked screen swallows taps meant for
     * the lock screen, so the app covers the picture and sends nothing until it is unlocked.
     */
    var touchOnLockScreen: Boolean
        get() = sp.getBoolean("touch_on_lock_screen", false)
        set(v) = sp.edit().putBoolean("touch_on_lock_screen", v).apply()

    var preferredTransport: Int
        get() = sp.getInt("preferred_transport", TRANSPORT_AUTO)
        set(v) = sp.edit().putInt("preferred_transport", v).apply()

    var bitrateKbps: Int
        get() = sp.getInt("bitrate_kbps", DEFAULT_BITRATE_KBPS)
            .coerceIn(MIN_BITRATE_KBPS, MAX_BITRATE_KBPS)
        set(v) = sp.edit().putInt("bitrate_kbps", v).apply()

    var fps: Int
        get() = sp.getInt("fps", DEFAULT_FPS)
        set(v) = sp.edit().putInt("fps", v).apply()

    var showStats: Boolean
        get() = sp.getBoolean("show_stats", false)
        set(v) = sp.edit().putBoolean("show_stats", v).apply()

    /**
     * Whether fingers are sent to the computer at all. Off leaves only the pen,
     * which is what people drawing with a resting hand want (SuperDisplay has
     * the same switch). Applies at once and stays the tablet's own business.
     */
    /**
     * In graphics-tablet mode nothing is streamed, and the screen would show
     * the same explanation for as long as the pen is in use — hours of static
     * text on an OLED panel. On, the explanation stays for a few seconds and
     * the screen then goes fully black; pen and touch keep working.
     */
    var blankPenOnly: Boolean
        get() = sp.getBoolean("blank_pen_only", true)
        set(v) = sp.edit().putBoolean("blank_pen_only", v).apply()

    var touchEnabled: Boolean
        get() = sp.getBoolean("touch_enabled", true)
        set(v) = sp.edit().putBoolean("touch_enabled", v).apply()

    /** `display`, `touchpad` or `stylus`: exactly one is active, synced with the computer. */
    var inputMode: String
        get() = sp.getString("input_mode", null)
            ?: if (sp.getBoolean("touchpad_mode", false)) HostConfig.MODE_TOUCHPAD else HostConfig.MODE_DISPLAY
        set(v) = sp.edit().putString("input_mode", v).apply()

    /** Battery saver also locks the screen with Device Admin (off: dim and stop retrying only). */
    var batterySaverLock: Boolean
        get() = sp.getBoolean("battery_saver_lock", false)
        set(v) = sp.edit().putBoolean("battery_saver_lock", v).apply()

    /** The computer has a Wi-Fi address for this tablet (so the pairing button stays hidden). */
    var wifiConfigured: Boolean
        get() = sp.getBoolean("wifi_configured", false)
        set(v) = sp.edit().putBoolean("wifi_configured", v).apply()

    /**
     * After the host has been unreachable for two minutes: stop retrying, dim to 10% and lock the
     * tablet. Synced with the computer (global default, per-device override).
     */
    var batterySaverOnDisconnect: Boolean
        get() = sp.getBoolean("battery_saver_on_disconnect", false)
        set(v) = sp.edit().putBoolean("battery_saver_on_disconnect", v).apply()

    /**
     * Opt-in immersive mode: hides the settings handle, expands into cutout,
     * and maps the Android Back button to toggle the settings sheet.
     * Default is false (OFF), matching original APK behavior.
     */
    var immersiveModeEnabled: Boolean
        get() = sp.getBoolean("immersive_mode_enabled", false)
        set(v) = sp.edit().putBoolean("immersive_mode_enabled", v).apply()

    /**
     * Which way round the tablet is held: [ORIENTATION_AUTO] follows the
     * sensor between the two landscape directions, the other two pin it.
     * Pinning exists because the sensor path does not work everywhere — a
     * Galaxy Tab S9 Ultra with auto-rotate on never left "camera up" — and
     * because people who draw with the camera at the bottom do not want the
     * picture flipping when the tablet is lifted.
     */
    var orientation: Int
        get() = sp.getInt("orientation", ORIENTATION_AUTO)
        set(v) = sp.edit().putInt("orientation", v).apply()

    /**
     * The daemon's session token, delivered as an intent extra when it
     * launches the app over adb. Kept so a relaunch by hand within the same
     * daemon run still authenticates; a new daemon run hands out a new one.
     */
    var hostToken: String?
        get() = sp.getString("host_token", null)
        set(v) = sp.edit().putString("host_token", v).apply()

    /** Shown once, after the first time video actually arrived. */
    var thankedOnce: Boolean
        get() = sp.getBoolean("thanked_once", false)
        set(v) = sp.edit().putBoolean("thanked_once", v).apply()

    /**
     * Whether the socket and streaming pipeline stay connected when the app
     * is backgrounded. Default is false (OFF) to prevent battery drain.
     */
    var keepConnectedInBackground: Boolean
        get() = sp.getBoolean("keep_connected_in_background", false)
        set(v) = sp.edit().putBoolean("keep_connected_in_background", v).apply()

    /**
     * Keep virtual display active and awake when the physical device is locked.
     * Default is true (ON).
     */
    var keepDisplayAliveWhenLocked: Boolean
        get() = sp.getBoolean("keep_display_alive_when_locked", true)
        set(v) = sp.edit().putBoolean("keep_display_alive_when_locked", v).apply()


    /** Light vibration on touchpad clicks, synced with the computer. */
    var touchpadHaptics: Boolean
        get() = sp.getBoolean("touchpad_haptics", true)
        set(v) = sp.edit().putBoolean("touchpad_haptics", v).apply()

    /** `bottom_right_zone` or `two_finger_tap`, synced with the computer. */
    var rightClickMethod: String
        get() = sp.getString("right_click_method", HostConfig.RIGHT_CLICK_ZONE) ?: HostConfig.RIGHT_CLICK_ZONE
        set(v) = sp.edit().putString("right_click_method", v).apply()

    /** `full_screen`, `standard_rectangle` or `wide_rectangle`, synced with the computer. */
    var touchpadShape: String
        get() = sp.getString("touchpad_shape", TouchpadShape.FULL) ?: TouchpadShape.FULL
        set(v) = sp.edit().putString("touchpad_shape", v).apply()

    var stylusStyle: String
        get() = sp.getString("stylus_style", HostConfig.STYLUS_MOUSE) ?: HostConfig.STYLUS_MOUSE
        set(v) = sp.edit().putString("stylus_style", v).apply()

    var stylusButtonPrimary: String
        get() = sp.getString("stylus_button_primary", HostConfig.PEN_BUTTON_RIGHT) ?: HostConfig.PEN_BUTTON_RIGHT
        set(v) = sp.edit().putString("stylus_button_primary", v).apply()

    var stylusButtonSecondary: String
        get() = sp.getString("stylus_button_secondary", HostConfig.PEN_BUTTON_MIDDLE) ?: HostConfig.PEN_BUTTON_MIDDLE
        set(v) = sp.edit().putString("stylus_button_secondary", v).apply()
}
