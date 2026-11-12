package com.uscreen.input

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The tap detector that lets the tablet tick when the computer's libinput makes the click. */
class NativeTapDetectorTest {
    private fun p(id: Int, x: Float = 10f, y: Float = 10f) = NativeTapDetector.Point(id, x, y)

    /** A quick still touch with one, two or three fingers is a tap when the last finger lifts. */
    @Test
    fun a_short_still_touch_is_a_tap_for_one_to_three_fingers() {
        for (n in 1..3) {
            val d = NativeTapDetector()
            assertFalse(d.update((1..n).map { p(it) }, 1000))
            assertTrue("$n fingers", d.update(emptyList(), 1100))
        }
    }

    /** A drag, a long press and a four-finger touch are not taps. */
    @Test
    fun movement_a_long_touch_or_four_fingers_are_not_taps() {
        var d = NativeTapDetector()
        d.update(listOf(p(1, 10f, 10f)), 0)
        d.update(listOf(p(1, 20f, 10f)), 50)
        assertFalse("drag", d.update(emptyList(), 100))
        d = NativeTapDetector()
        d.update(listOf(p(1)), 0)
        assertFalse("long press", d.update(emptyList(), 600))
        d = NativeTapDetector()
        d.update((1..4).map { p(it) }, 0)
        assertFalse("four fingers", d.update(emptyList(), 100))
    }

    /** Lifting with nothing down, and two taps in a row, behave. */
    @Test
    fun an_empty_update_alone_is_not_a_tap_and_taps_repeat() {
        val d = NativeTapDetector()
        assertFalse(d.update(emptyList(), 0))
        d.update(listOf(p(1)), 100); assertTrue(d.update(emptyList(), 160))
        d.update(listOf(p(2)), 300); assertTrue(d.update(emptyList(), 350))
    }
}
