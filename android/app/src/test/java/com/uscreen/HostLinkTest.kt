package com.uscreen

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HostLinkTest {
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

    @Test
    fun retryIntervalIsTwoSecondsAndWindowTwoMinutes() {
        assertEquals(2_000L, ReconnectPolicy.RETRY_INTERVAL_MS)
        assertEquals(120_000L, ReconnectPolicy.OFFLINE_AFTER_MS)
    }

    @Test
    fun exponentialCurveStartsAtZeroEndsAtOneAndIsSlowFirst() {
        assertEquals(0f, FadeCurve.exponential(0f), 1e-6f)
        assertEquals(1f, FadeCurve.exponential(1f), 1e-6f)
        assertTrue(FadeCurve.exponential(0.5f) < 0.05f)
        assertTrue(FadeCurve.exponential(0.9f) > FadeCurve.exponential(0.5f))
        assertEquals(1f, FadeCurve.exponential(7f), 1e-6f)
    }

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

    @Test
    fun bezierCurveStringsAreParsedAndClamped() {
        assertEquals(listOf(0.25f, 0.1f, 0.25f, 1f), FadeCurve.parseBezier("bezier:0.25,0.1,0.25,1")?.toList())
        assertEquals(listOf(1f, -0.5f, 0f, 2f), FadeCurve.parseBezier("bezier:3,-0.5,-1,2")?.toList())
        assertEquals(null, FadeCurve.parseBezier("ease_in"))
        assertEquals(null, FadeCurve.parseBezier("bezier:1,2,3"))
        assertEquals(null, FadeCurve.parseBezier("bezier:a,b,c,d"))
    }

    @Test
    fun newSettingsFollowTheSameRulesAsTheOthers() {
        val base = HostConfig()
        assertEquals("standard", base.stylusProfile)
        assertTrue(base.touchpadHaptics)
        assertEquals(HostConfig.RIGHT_CLICK_ZONE, base.rightClickMethod)
        assertEquals("copy-pen", base.withSetting("stylus_profile", "copy-pen")?.stylusProfile)
        assertEquals(null, base.withSetting("stylus_profile", ""))
        assertEquals(false, base.withSetting("touchpad_haptics", false)?.touchpadHaptics)
        assertEquals(
            HostConfig.RIGHT_CLICK_TWO_FINGER,
            base.withSetting("right_click_method", "two_finger_tap")?.rightClickMethod
        )
        assertEquals(null, base.withSetting("right_click_method", "triple_tap"))
        val o = org.json.JSONObject(
            """{"stylus_profile":"p","touchpad_haptics":false,"right_click_method":"two_finger_tap"}"""
        )
        val c = HostConfig.fromJson(o, base)
        assertEquals("p", c.stylusProfile)
        assertFalse(c.touchpadHaptics)
        assertEquals("two_finger_tap", c.rightClickMethod)
        // A message without the keys, or with a bad method, leaves the base alone.
        assertEquals(base.rightClickMethod, HostConfig.fromJson(org.json.JSONObject("""{"right_click_method":"x"}"""), base).rightClickMethod)
    }

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

    @Test
    fun stylusCatalogParsesTheHostsList() {
        val c = StylusCatalog.fromJson(
            org.json.JSONObject(
                """{"type":"stylus_profiles",
                    "profiles":[{"id":"standard","name":"Standard","builtin":true,"mappings":{}},
                                {"id":"copy","name":"Copy","builtin":false,"mappings":{"button_primary":"ctrl+c"}}],
                    "triggers":[{"key":"button_primary","label":"Press side button"}]}"""
            )
        )
        assertEquals(2, c.profiles.size)
        assertEquals("ctrl+c", c.profiles[1].mappings["button_primary"])
        assertFalse(c.profiles[1].builtin)
        assertEquals("button_primary", c.triggers.single().key)
    }

    @Test
    fun stylusActionsAreCheckedLikeTheHostDoes() {
        for (ok in listOf("", "mouse_middle", "ctrl+c", "Ctrl+Shift+Z", "meta+pageup", "f5", "alt+f4")) {
            assertTrue(ok, StylusActions.isValid(ok))
        }
        for (bad in listOf("ctrl+", "hyper+c", "ctrl+nonsense", "ctrl+cc", "+c")) {
            assertFalse(bad, StylusActions.isValid(bad))
        }
    }
}
