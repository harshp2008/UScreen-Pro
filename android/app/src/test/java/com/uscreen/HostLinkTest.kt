package com.uscreen

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Tests of the reconnect policy, the fade curves, charging text and the host-to-tablet settings model.
 */
class HostLinkTest {
    /** The host counts as offline only after the full 120 s window of consecutive failures. */
    @Test
    fun hostIsOnlyOfflineAfterTheFullWindowOfFailures() {
        val p = ReconnectPolicy()
        p.linkDown(0)
        assertFalse(p.expired(0))
        assertFalse(p.expired(2_000))
        assertFalse(p.expired(119_999))
        assertTrue(p.expired(120_000))
        assertTrue(p.expired(500_000))
    }

    /** One success resets the clock; later failures inside one streak do not restart it. */
    @Test
    fun oneSuccessResetsTheClock() {
        val p = ReconnectPolicy()
        p.linkDown(0)
        p.linkDown(100_000) // later failures do not restart a streak
        assertEquals(100_000, p.downForMs(100_000))
        p.linkUp()
        assertEquals(0, p.downForMs(110_000))
        p.linkDown(110_000)
        assertFalse(p.expired(200_000))
        assertTrue(p.expired(230_000))
    }

    /** The retry interval is 2 s and the offline window is 2 min. */
    @Test
    fun retryIntervalIsTwoSecondsAndWindowTwoMinutes() {
        assertEquals(2_000L, ReconnectPolicy.RETRY_INTERVAL_MS)
        assertEquals(120_000L, ReconnectPolicy.OFFLINE_AFTER_MS)
    }

    /**
     * The exponential curve starts at 0, ends at exactly 1, is slow at first and clamps input above 1.
     */
    @Test
    fun exponentialCurveStartsAtZeroEndsAtOneAndIsSlowFirst() {
        assertEquals(0f, FadeCurve.exponential(0f), 1e-6f)
        assertEquals(1f, FadeCurve.exponential(1f), 1e-6f)
        assertTrue(FadeCurve.exponential(0.5f) < 0.05f)
        assertTrue(FadeCurve.exponential(0.9f) > FadeCurve.exponential(0.5f))
        assertEquals(1f, FadeCurve.exponential(7f), 1e-6f)
    }

    /**
     * Charging text follows plugged state, status and the current reading, including milliamp-reporting devices.
     */
    @Test
    fun chargingTextFollowsTheReading() {
        val usb = android.os.BatteryManager.BATTERY_PLUGGED_USB
        val ac = android.os.BatteryManager.BATTERY_PLUGGED_AC
        assertEquals("On battery", ChargingStatus.describe(0, 3, 450_000))
        assertEquals("Charging: 450mA", ChargingStatus.describe(usb, 2, -450_000))
        assertEquals("Charging: 450mA", ChargingStatus.describe(usb, 2, 450))
        assertEquals("Charging (Slow): 120mA", ChargingStatus.describe(usb, 2, 120_000))
        assertEquals("Charging (Slow)", ChargingStatus.describe(usb, 2, null))
        assertEquals("Charging", ChargingStatus.describe(ac, 2, 0))
        assertEquals("Fully charged", ChargingStatus.describe(ac, android.os.BatteryManager.BATTERY_STATUS_FULL, 10_000))
    }

    /**
     * Tablet-side settings map to `HostConfig` fields; an unknown input mode is rejected, and touchpad mode stops video streaming.
     */
    @Test
    fun tabletSettingsMapToHostConfigFields() {
        val base = HostConfig()
        assertEquals("touchpad", base.withSetting("input_mode", "touchpad")?.inputMode)
        assertTrue(base.withSetting("input_mode", "touchpad")?.streamsVideo == false)
        assertTrue(base.streamsVideo)
        assertEquals(null, base.withSetting("input_mode", "flying"))
        assertEquals(true, base.withSetting("show_stats", true)?.showStats)
        assertEquals("camera_up", base.withSetting("orientation", "camera_up")?.orientation)
        assertEquals(true, base.withSetting("battery_saver_lock", true)?.batterySaverLock)
        assertEquals(null, base.withSetting("show_stats", "yes"))
        assertEquals(null, base.withSetting("nonsense", true))
    }

    /**
     * `bezier:` curve strings are parsed and their control points clamped; malformed strings give null.
     */
    @Test
    fun bezierCurveStringsAreParsedAndClamped() {
        assertEquals(listOf(0.25f, 0.1f, 0.25f, 1f), FadeCurve.parseBezier("bezier:0.25,0.1,0.25,1")?.toList())
        assertEquals(listOf(1f, -0.5f, 0f, 2f), FadeCurve.parseBezier("bezier:3,-0.5,-1,2")?.toList())
        assertEquals(null, FadeCurve.parseBezier("ease_in"))
        assertEquals(null, FadeCurve.parseBezier("bezier:1,2,3"))
        assertEquals(null, FadeCurve.parseBezier("bezier:a,b,c,d"))
    }

    /** Haptic strength and click sound: defaults, validation and JSON. */
    @Test
    fun hapticStrengthAndClickSoundAreValidated() {
        val base = HostConfig()
        assertEquals("medium", base.touchpadHapticStrength)
        assertFalse(base.touchpadClickSound)
        assertEquals("strong", base.withSetting("touchpad_haptic_strength", "strong")?.touchpadHapticStrength)
        assertEquals(null, base.withSetting("touchpad_haptic_strength", "extreme"))
        assertTrue(base.withSetting("touchpad_click_sound", true)?.touchpadClickSound == true)
        val c = HostConfig.fromJson(org.json.JSONObject("""{"touchpad_haptic_strength":"light","touchpad_click_sound":true}"""), base)
        assertEquals("light", c.touchpadHapticStrength)
        assertTrue(c.touchpadClickSound)
        assertEquals("medium", HostConfig.fromJson(org.json.JSONObject("""{"touchpad_haptic_strength":"x"}"""), base).touchpadHapticStrength)
    }

    /**
     * The newer settings (touchpad haptics, right-click method) follow the same rules as the older ones: defaults, validation and JSON.
     */
    @Test
    fun newSettingsFollowTheSameRulesAsTheOthers() {
        val base = HostConfig()
        assertTrue(base.touchpadHaptics)
        assertEquals(HostConfig.RIGHT_CLICK_ZONE, base.rightClickMethod)
        assertEquals(false, base.withSetting("touchpad_haptics", false)?.touchpadHaptics)
        assertEquals(
            HostConfig.RIGHT_CLICK_TWO_FINGER,
            base.withSetting("right_click_method", "two_finger_tap")?.rightClickMethod
        )
        assertEquals(null, base.withSetting("right_click_method", "triple_tap"))
        val o = org.json.JSONObject(
            """{"touchpad_haptics":false,"right_click_method":"two_finger_tap"}"""
        )
        val c = HostConfig.fromJson(o, base)
        assertFalse(c.touchpadHaptics)
        assertEquals("two_finger_tap", c.rightClickMethod)
        // A message without the keys, or with a bad method, leaves the base alone.
        assertEquals(base.rightClickMethod, HostConfig.fromJson(org.json.JSONObject("""{"right_click_method":"x"}"""), base).rightClickMethod)
    }

    /**
     * Stylus settings are validated: style, primary and secondary button names, with defaults auto, right and middle.
     */
    @Test
    fun stylusSettingsAreValidated() {
        val base = HostConfig()
        assertEquals(HostConfig.STYLUS_AUTO, base.stylusStyle)
        assertEquals(HostConfig.PEN_BUTTON_RIGHT, base.stylusButtonPrimary)
        assertEquals(HostConfig.PEN_BUTTON_MIDDLE, base.stylusButtonSecondary)
        assertEquals("auto", base.withSetting("stylus_style", "auto")?.stylusStyle)
        assertEquals("tablet", base.withSetting("stylus_style", "tablet")?.stylusStyle)
        assertEquals(null, base.withSetting("stylus_style", "wacom"))
        assertEquals("none", base.withSetting("stylus_button_primary", "none")?.stylusButtonPrimary)
        assertEquals(null, base.withSetting("stylus_button_secondary", "left"))
        val c = HostConfig.fromJson(
            org.json.JSONObject("""{"stylus_style":"tablet","stylus_button_primary":"middle","stylus_button_secondary":"bogus"}"""),
            base,
        )
        assertEquals("tablet", c.stylusStyle)
        assertEquals("middle", c.stylusButtonPrimary)
        assertEquals(base.stylusButtonSecondary, c.stylusButtonSecondary)
    }

    /** The touchpad shape is a setting: default full screen, validated, and read from JSON. */
    @Test
    fun touchpadShapeIsASetting() {
        val base = HostConfig()
        assertEquals(TouchpadShape.FULL, base.touchpadShape)
        assertEquals(TouchpadShape.WIDE, base.withSetting("touchpad_shape", "wide_rectangle")?.touchpadShape)
        assertEquals(null, base.withSetting("touchpad_shape", "circle"))
        val c = HostConfig.fromJson(org.json.JSONObject("""{"touchpad_shape":"standard_rectangle"}"""), base)
        assertEquals(TouchpadShape.STANDARD, c.touchpadShape)
        assertEquals(TouchpadShape.FULL, HostConfig.fromJson(org.json.JSONObject("""{"touchpad_shape":"x"}"""), base).touchpadShape)
    }

    /** Native gestures default to on, accept only booleans, and follow the host's JSON. */
    @Test
    fun nativeGesturesAreOnByDefaultAndFollowTheHost() {
        val base = HostConfig()
        assertEquals(true, base.nativeGestures)
        assertEquals(false, base.withSetting("native_gestures", false)?.nativeGestures)
        assertEquals(null, base.withSetting("native_gestures", "yes"))
        val off = HostConfig.fromJson(org.json.JSONObject("""{"native_gestures":false}"""), base)
        assertEquals(false, off.nativeGestures)
        // A config_update without the field (an older host) leaves the value alone.
        assertEquals(true, HostConfig.fromJson(org.json.JSONObject("""{"touchpad_shape":"full_screen"}"""), base).nativeGestures)
    }

    /** Touchpad sensitivity defaults to 100 and is clamped to its range (50 to 200). */
    @Test
    fun touchpadSensitivityDefaultsTo100AndStaysInRange() {
        val base = HostConfig()
        assertEquals(100, base.touchpadSensitivity)
        assertEquals(150, base.withSetting("touchpad_sensitivity", 150)?.touchpadSensitivity)
        assertEquals(200, base.withSetting("touchpad_sensitivity", 900)?.touchpadSensitivity)
        assertEquals(50, base.withSetting("touchpad_sensitivity", 1)?.touchpadSensitivity)
        assertEquals(null, base.withSetting("touchpad_sensitivity", "fast"))
        assertEquals(120, HostConfig.fromJson(org.json.JSONObject("""{"touchpad_sensitivity":120}"""), base).touchpadSensitivity)
        assertEquals(100, HostConfig.fromJson(org.json.JSONObject("""{"touchpad_shape":"full_screen"}"""), base).touchpadSensitivity)
    }

    /**
     * The rectangle shapes are centred and as large as their aspect ratio allows inside the screen (checked on 2560 x 1600).
     */
    @Test
    fun theRectanglesAreCentredAndAsLargeAsTheirRatioAllows() {
        // The tablet itself: 2560x1600 (16:10).
        val std = TouchpadShape.bounds(TouchpadShape.STANDARD, 2560f, 1600f)
        assertEquals(1600f, std.height, 0.01f)
        assertEquals(1600f * 4f / 3f, std.width, 0.01f)
        assertEquals((2560f - std.width) / 2f, std.left, 0.01f)
        assertEquals(0f, std.top, 0.01f)
        val wide = TouchpadShape.bounds(TouchpadShape.WIDE, 2560f, 1600f)
        assertEquals(2560f, wide.width, 0.01f)
        assertEquals(2560f * 9f / 16f, wide.height, 0.01f)
        assertEquals((1600f - wide.height) / 2f, wide.top, 0.01f)
        // A narrow (portrait) surface keeps its width and loses height instead.
        val tall = TouchpadShape.bounds(TouchpadShape.STANDARD, 1000f, 2000f)
        assertEquals(1000f, tall.width, 0.01f)
        assertEquals(750f, tall.height, 0.01f)
        assertEquals(TouchpadShape.Bounds(0f, 0f, 2560f, 1600f), TouchpadShape.bounds(TouchpadShape.FULL, 2560f, 1600f))
        assertTrue(std.contains(1280f, 800f))
        assertFalse(std.contains(10f, 800f))
    }
}
