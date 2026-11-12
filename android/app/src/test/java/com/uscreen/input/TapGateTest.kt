package com.uscreen.input

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The gate that drops taps in the Classic touchpad without tap-to-click. */
class TapGateTest {
    private fun frame(vararg p: Pair<Int, Float>) = p.map { NativeTapDetector.Point(it.first, it.second, 10f) }
    private fun gate() = TapGate<List<NativeTapDetector.Point>>({ it })

    /** A short still touch never reaches the computer: neither its fingers nor its lift. */
    @Test
    fun a_still_tap_is_dropped_whole() {
        val g = gate()
        assertTrue(g.offer(frame(1 to 10f), 0).isEmpty())
        assertTrue(g.offer(frame(1 to 10.5f), 40).isEmpty())
        assertTrue(g.offer(emptyList(), 90).isEmpty())
    }

    /** A touch that moves is let through, held frames first and in order, and its lift goes through too. */
    @Test
    fun a_moving_touch_releases_the_held_frames_in_order() {
        val g = gate()
        val a = frame(1 to 10f)
        val b = frame(1 to 11f)
        val c = frame(1 to 30f)
        assertTrue(g.offer(a, 0).isEmpty())
        assertTrue(g.offer(b, 20).isEmpty())
        assertEquals(listOf(a, b, c), g.offer(c, 40))
        val d = frame(1 to 40f)
        assertEquals(listOf(d), g.offer(d, 60))
        val lift = emptyList<NativeTapDetector.Point>()
        assertEquals(listOf(lift), g.offer(lift, 80))
    }

    /** A finger resting longer than a tap is let through (a press and hold), and the next touch starts fresh. */
    @Test
    fun a_long_touch_is_let_through_and_the_next_tap_is_dropped_again() {
        val g = gate()
        val a = frame(1 to 10f)
        assertTrue(g.offer(a, 0).isEmpty())
        val b = frame(1 to 10f)
        assertEquals(listOf(a, b), g.offer(b, 400))
        g.offer(emptyList(), 500)
        assertTrue(g.offer(frame(2 to 50f), 1000).isEmpty())
        assertTrue(g.offer(emptyList(), 1050).isEmpty())
    }

    /** Two fingers that scroll (move apart in step) are let through as soon as either moves. */
    @Test
    fun two_moving_fingers_are_let_through() {
        val g = gate()
        assertTrue(g.offer(frame(1 to 10f, 2 to 20f), 0).isEmpty())
        assertEquals(2, g.offer(frame(1 to 10f, 2 to 40f), 30).size)
    }
}
