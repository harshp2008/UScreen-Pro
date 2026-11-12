package com.uscreen

import kotlin.math.abs
import kotlin.math.hypot
import kotlin.random.Random
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The engine fed with input shaped like a real hand, not like a script: 120 Hz samples, jitter,
 * contact size, fingers that land and lift a few tens of milliseconds apart. The first version of
 * this engine passed tidy synthetic tests and failed on a real hand, so these deliberately are not
 * tidy.
 */
class TouchpadEngineTest {
    /**
     * Records everything the engine outputs: totals of movement, scroll and pinch, the swipes and an ordered event log.
     */
    private class Rec : TouchpadEngine.Output {
        val events = ArrayList<String>()
        var moveX = 0f; var moveY = 0f
        var scrollX = 0f; var scrollY = 0f
        var pinchTotal = 0f
        var moves = 0
        val swipes = ArrayList<Pair<Int, TouchpadEngine.Swipe>>()
        /** Adds the movement to the totals and logs `move`. */
        override fun move(dxMm: Float, dyMm: Float) { moveX += dxMm; moveY += dyMm; moves++; events.add("move") }
        /** Logs a button as `b<n>v` (down) or `b<n>^` (up). */
        override fun button(button: Int, down: Boolean) { events.add("b$button${if (down) "v" else "^"}") }
        /** Adds the scroll to the totals and logs `scroll`. */
        override fun scroll(dxMm: Float, dyMm: Float) { scrollX += dxMm; scrollY += dyMm; events.add("scroll") }
        /** Adds the pinch to the total and logs `pinch`. */
        override fun pinch(deltaMm: Float) { pinchTotal += deltaMm; events.add("pinch") }
        /** Logs the end of the pinch. */
        override fun pinchEnd() { events.add("pinchEnd") }
        /** Records the swipe and logs it. */
        override fun swipe(fingers: Int, direction: TouchpadEngine.Swipe) { swipes.add(fingers to direction); events.add("swipe$fingers$direction") }
        /** The button events in order. */
        fun buttons() = events.filter { it.startsWith("b") }
        /** How many logged events start with [prefix]. */
        fun count(prefix: String) = events.count { it.startsWith(prefix) }
    }

    /**
     * A simulated touch surface (200 x 125 mm, right-click zone on) that drives the engine with frames and a fake clock and runs its timer when due.
     */
    private class Sim {
        val rec = Rec()
        var timerAt = -1.0
        var now = 1000.0
        val engine = TouchpadEngine(rec) { delayMs -> timerAt = now + delayMs / 1000.0 }.also {
            it.padWidthMm = 200f; it.padHeightMm = 125f; it.rightClickZone = true
        }
        private val rnd = Random(7)
        val fingers = LinkedHashMap<Int, FloatArray>() // id -> x, y, major, pressure

        /** A small random offset in millimetres, within +-0.15 mm, to mimic sensor noise. */
        fun jitter() = (rnd.nextFloat() - 0.5f) * 0.3f

        /** Advance [dt] seconds, running the engine's timer if it falls due. */
        fun tick(dt: Double) {
            val end = now + dt
            if (timerAt in 0.0..end && timerAt > now) {
                now = timerAt; timerAt = -1.0; engine.onTimer(now)
            }
            now = end
        }

        /** Sends the current fingers to the engine as one frame. */
        fun frame() {
            engine.onFrame(HardwareState(now, fingers.map { (id, v) -> FingerState(id, v[0], v[1], v[2], v[2] * 0.8f, v[3]) }))
        }

        /** Puts finger [id] down at ([x], [y]) with contact length [major]. */
        fun down(id: Int, x: Float, y: Float, major: Float = 9f) {
            fingers[id] = floatArrayOf(x, y, major, 0.3f); frame()
        }

        /** Lifts finger [id]. */
        fun up(id: Int) { fingers.remove(id); frame() }

        /** Move the listed fingers by (dx, dy) in total over [seconds], sampled at 120 Hz with jitter. */
        fun drag(ids: List<Int>, dx: Float, dy: Float, seconds: Double, noise: Boolean = true) {
            val steps = (seconds * 120).toInt().coerceAtLeast(1)
            val startPos = ids.associateWith { fingers[it]!!.copyOf() }
            for (s in 1..steps) {
                tick(1.0 / 120)
                for (id in ids) {
                    val st = startPos[id]!!
                    fingers[id]!![0] = st[0] + dx * s / steps + if (noise) jitter() else 0f
                    fingers[id]!![1] = st[1] + dy * s / steps + if (noise) jitter() else 0f
                }
                frame()
            }
        }

        /** Holds the fingers still for [seconds], with a little noise, one frame per 1/120 s. */
        fun hold(seconds: Double) {
            val steps = (seconds * 120).toInt()
            for (s in 0 until steps) {
                tick(1.0 / 120)
                for (v in fingers.values) { v[0] += jitter() * 0.3f; v[1] += jitter() * 0.3f }
                frame()
            }
        }
    }

    @Test fun `slow drag with jitter moves smoothly and never stalls`() {
        val s = Sim()
        s.down(1, 50f, 60f)
        s.hold(0.05)
        // 40 mm in 4 s is 10 mm/s, about 0.08 mm per sample, well under the report distance.
        var last = s.rec.moves
        var longestGap = 0; var gap = 0
        val steps = 4 * 120
        for (i in 1..steps) {
            s.tick(1.0 / 120)
            s.fingers[1]!![0] = 50f + 40f * i / steps + s.jitter()
            s.fingers[1]!![1] = 60f + s.jitter()
            s.frame()
            // The first quarter second is the tap window: motion is held back until a tap is ruled out.
            if (i < 45) { last = s.rec.moves; continue }
            if (s.rec.moves == last) gap++ else { longestGap = maxOf(longestGap, gap); gap = 0; last = s.rec.moves }
        }
        s.up(1); s.tick(0.5)
        assertEquals("distance moved", 40f, s.rec.moveX, 1.5f)
        assertEquals(0f, s.rec.moveY, 1.5f)
        assertTrue("longest stall $longestGap frames", longestGap <= 12) // 0.35 mm at 10 mm/s is 4 frames
        assertEquals("no stray click", 0, s.rec.buttons().size)
    }

    @Test fun `a short tap is one left click`() {
        val s = Sim()
        s.down(1, 80f, 50f); s.hold(0.09); s.up(1); s.tick(0.4)
        assertEquals(listOf("b0v", "b0^"), s.rec.buttons())
    }

    @Test fun `a long press is no click`() {
        val s = Sim()
        s.down(1, 80f, 50f); s.hold(0.6); s.up(1); s.tick(0.5)
        assertEquals(0, s.rec.buttons().size)
    }

    @Test fun `a finger that slides is no click`() {
        val s = Sim()
        s.down(1, 80f, 50f); s.drag(listOf(1), 25f, 0f, 0.25); s.up(1); s.tick(0.5)
        assertEquals(0, s.rec.buttons().size)
        assertEquals(25f, s.rec.moveX, 1.5f)
    }

    @Test fun `tap then touch and move drags with the button held`() {
        val s = Sim()
        s.down(1, 60f, 50f); s.hold(0.07); s.up(1)
        s.tick(0.08)
        s.down(1, 60f, 50f)
        s.drag(listOf(1), 40f, 10f, 0.6)
        val before = s.rec.events.indexOf("b0v")
        s.up(1); s.tick(0.5)
        assertEquals(listOf("b0v", "b0^"), s.rec.buttons())
        val firstMove = s.rec.events.indexOf("move")
        assertTrue("button goes down before the pointer moves", before in 0 until firstMove)
        assertEquals(40f, s.rec.moveX, 2f)
        assertTrue("released after the last move", s.rec.events.lastIndexOf("b0^") > s.rec.events.lastIndexOf("move"))
    }

    @Test fun `two finger tap is a right click`() {
        val s = Sim()
        s.down(1, 80f, 50f); s.tick(0.02); s.down(2, 100f, 52f)
        s.hold(0.06); s.up(1); s.tick(0.015); s.up(2); s.tick(0.5)
        assertEquals(listOf("b1v", "b1^"), s.rec.buttons())
        assertEquals(0, s.rec.moves)
    }

    @Test fun `three finger tap with staggered fingers is a middle click`() {
        val s = Sim()
        s.down(1, 70f, 50f); s.tick(0.015); s.down(2, 90f, 52f); s.tick(0.02); s.down(3, 110f, 51f)
        s.hold(0.05); s.up(2); s.tick(0.01); s.up(1); s.tick(0.012); s.up(3); s.tick(0.5)
        assertEquals(listOf("b2v", "b2^"), s.rec.buttons())
    }

    @Test fun `two fingers moving together scroll and do not click`() {
        val s = Sim()
        s.down(1, 80f, 50f); s.tick(0.01); s.down(2, 100f, 50f)
        s.drag(listOf(1, 2), 0f, 30f, 0.5)
        s.up(1); s.tick(0.01); s.up(2); s.tick(0.5)
        assertEquals(30f, s.rec.scrollY, 3f)
        assertEquals(0, s.rec.buttons().size)
        assertEquals(0, s.rec.count("pinch"))
        assertEquals(0, s.rec.moves)
    }

    @Test fun `fingers moving apart pinch and release the pinch`() {
        val s = Sim()
        s.down(1, 90f, 60f); s.tick(0.01); s.down(2, 110f, 60f)
        s.hold(0.03)
        val a = s.fingers[1]!!; val b = s.fingers[2]!!
        val startDist = hypot(a[0] - b[0], a[1] - b[1])
        val steps = 60
        for (i in 1..steps) {
            s.tick(1.0 / 120)
            a[0] = 90f - 20f * i / steps + s.jitter(); b[0] = 110f + 20f * i / steps + s.jitter()
            s.frame()
        }
        val endDist = hypot(a[0] - b[0], a[1] - b[1])
        s.up(1); s.tick(0.01); s.up(2); s.tick(0.5)
        assertEquals("distance gained", endDist - startDist, s.rec.pinchTotal, 4f)
        assertTrue(s.rec.pinchTotal > 25f)
        assertEquals(1, s.rec.count("pinchEnd"))
        assertEquals(0, s.rec.buttons().size)
        assertTrue("a pinch is not a scroll", abs(s.rec.scrollX) + abs(s.rec.scrollY) < 3f)
    }

    @Test fun `four finger swipe up is reported once`() {
        val s = Sim()
        s.down(1, 60f, 80f); s.tick(0.012); s.down(2, 85f, 82f); s.tick(0.015)
        s.down(3, 110f, 81f); s.tick(0.01); s.down(4, 135f, 83f)
        s.hold(0.03)
        s.drag(listOf(1, 2, 3, 4), 2f, -45f, 0.4)
        s.up(1); s.up(2); s.up(3); s.up(4); s.tick(0.5)
        assertEquals(listOf(4 to TouchpadEngine.Swipe.UP), s.rec.swipes)
        assertEquals(0, s.rec.buttons().size)
    }

    @Test fun `three finger swipe left is a workspace swipe`() {
        val s = Sim()
        s.down(1, 120f, 60f); s.tick(0.02); s.down(2, 140f, 62f); s.tick(0.02); s.down(3, 160f, 61f)
        s.hold(0.03)
        s.drag(listOf(1, 2, 3), -50f, 2f, 0.4)
        s.up(1); s.up(2); s.up(3); s.tick(0.5)
        assertEquals(listOf(3 to TouchpadEngine.Swipe.LEFT), s.rec.swipes)
    }

    @Test fun `fingers going opposite ways in a swipe are not a swipe`() {
        val s = Sim()
        s.down(1, 60f, 80f); s.down(2, 85f, 80f); s.down(3, 110f, 80f)
        val a = s.fingers[1]!!; val b = s.fingers[2]!!; val c = s.fingers[3]!!
        for (i in 1..40) {
            s.tick(1.0 / 120)
            a[1] = 80f - i * 0.5f; b[1] = 80f; c[1] = 80f + i * 0.5f
            s.frame()
        }
        assertEquals(0, s.rec.swipes.size)
    }

    @Test fun `a palm sized contact does nothing`() {
        val s = Sim()
        s.down(1, 100f, 60f, major = 28f)
        s.drag(listOf(1), 30f, 5f, 0.4)
        s.up(1); s.tick(0.5)
        assertTrue(s.rec.events.isEmpty())
    }

    @Test fun `a palm resting next to a working finger is ignored`() {
        val s = Sim()
        s.down(2, 150f, 100f, major = 30f)
        s.tick(0.01)
        s.down(1, 60f, 50f)
        s.drag(listOf(1), 30f, 0f, 0.4)
        s.up(1); s.up(2); s.tick(0.5)
        assertEquals(30f, s.rec.moveX, 2f)
        assertEquals(0, s.rec.buttons().size)
        assertEquals(0, s.rec.count("scroll"))
    }

    @Test fun `a thumb holding the edge does nothing`() {
        val s = Sim()
        s.down(1, 3f, 60f)
        s.hold(0.1); s.up(1); s.tick(0.5)
        assertTrue(s.rec.events.isEmpty())
    }

    @Test fun `tap in the bottom right zone is a right click`() {
        val s = Sim()
        s.down(1, 170f, 115f); s.hold(0.08); s.up(1); s.tick(0.4)
        assertEquals(listOf("b1v", "b1^"), s.rec.buttons())
    }

    @Test fun `tap in the bottom left corner is a left click`() {
        val s = Sim()
        s.down(1, 50f, 115f); s.hold(0.08); s.up(1); s.tick(0.4)
        assertEquals(listOf("b0v", "b0^"), s.rec.buttons())
    }

    @Test fun `lifting one scrolling finger does not make the pointer jump`() {
        val s = Sim()
        s.down(1, 80f, 50f); s.tick(0.01); s.down(2, 100f, 50f)
        s.drag(listOf(1, 2), 0f, 20f, 0.3)
        s.up(2)
        val m0 = s.rec.moves
        s.drag(listOf(1), 8f, 0f, 0.1) // the leftover finger drifts while the hand lifts
        assertEquals("no pointer motion right after a lift", m0, s.rec.moves)
        s.up(1); s.tick(0.5)
        assertEquals(0, s.rec.buttons().size)
    }

    @Test fun `cancel lets go of a held drag button`() {
        val s = Sim()
        s.down(1, 60f, 50f); s.hold(0.07); s.up(1); s.tick(0.08)
        s.down(1, 60f, 50f); s.drag(listOf(1), 20f, 0f, 0.3)
        s.engine.cancel()
        assertEquals(listOf("b0v", "b0^"), s.rec.buttons())
    }
}
