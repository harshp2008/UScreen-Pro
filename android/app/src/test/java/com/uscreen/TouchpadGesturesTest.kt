package com.uscreen

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TouchpadGesturesTest {
    private val log = mutableListOf<String>()
    private val out = object : TouchpadGestures.Output {
        override fun move(dx: Float, dy: Float) { log += "move ${dx.toInt()},${dy.toInt()}" }
        override fun button(button: Int, down: Boolean) { log += "button $button ${if (down) "down" else "up"}" }
        override fun scroll(dx: Float, dy: Float) { log += "scroll ${dx.toInt()},${dy.toInt()}" }
        override fun transform(zoom: Float, rotationDeg: Float) {
            log += "transform ${"%.2f".format(zoom)} ${"%.0f".format(rotationDeg)}"
        }
        override fun transformEnd() { log += "transform end" }
        override fun workspace(direction: Int) { log += "workspace $direction" }
    }

    /** A 1000x600 surface: the right-click strip is y >= 510, x >= 500. */
    private fun pad(gain: Float = 1f, natural: Boolean = true, zone: Boolean = false) =
        TouchpadGestures(out, gain, natural).also {
            it.rightClickZone = zone
            it.setBounds(0f, 0f, 1000f, 600f)
        }

    @Test fun quickTapIsALeftClick() {
        val g = pad()
        g.down(0, 100f, 100f, 0); g.up(0, 100)
        assertEquals(listOf("button 0 down", "button 0 up"), log)
    }

    @Test fun slowTouchIsNotAClick() {
        val g = pad()
        g.down(0, 100f, 100f, 0); g.up(0, 600)
        assertTrue(log.isEmpty())
    }

    @Test fun movingPastTheDeadZoneSendsEverythingSinceTouchdown() {
        val g = pad()
        g.down(0, 100f, 100f, 0)
        g.move(0, 105f, 100f)          // inside the dead zone
        g.move(0, 120f, 100f)          // out: 20 px since touchdown
        g.move(0, 130f, 105f)
        g.up(0, 40)
        assertEquals(listOf("move 20,0", "move 10,5"), log)
    }

    @Test fun gainScalesMovement() {
        val g = pad(gain = 2f)
        g.down(0, 0f, 0f, 0); g.move(0, 20f, 0f); g.move(0, 30f, 0f)
        assertEquals(listOf("move 40,0", "move 20,0"), log)
    }

    @Test fun tapThenTouchAndMoveDrags() {
        val g = pad()
        g.down(0, 50f, 50f, 0); g.up(0, 80)           // click
        log.clear()
        g.down(0, 50f, 50f, 200)
        g.move(0, 80f, 50f)
        g.move(0, 90f, 50f)
        g.up(0, 260)
        assertEquals(listOf("button 0 down", "move 30,0", "move 10,0", "button 0 up"), log)
    }

    @Test fun tapThenTapAgainIsTwoClicks() {
        val g = pad()
        g.down(0, 50f, 50f, 0); g.up(0, 80)
        g.down(0, 50f, 50f, 200); g.up(0, 260)
        assertEquals(List(2) { listOf("button 0 down", "button 0 up") }.flatten(), log)
    }

    @Test fun aLateSecondTouchIsNotADrag() {
        val g = pad()
        g.down(0, 50f, 50f, 0); g.up(0, 80)
        log.clear()
        g.down(0, 50f, 50f, 1000)
        g.move(0, 90f, 50f)
        assertEquals(listOf("move 40,0"), log)
    }

    @Test fun twoFingersMovingScrollWithTheAverage() {
        val g = pad()
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 5)
        g.move(0, 100f, 130f)          // a lone finger could still be a turn: nothing yet
        assertTrue(log.isEmpty())
        g.move(1, 200f, 130f)          // both moved the same way: a scroll, with what was travelled so far
        g.move(1, 200f, 150f)
        assertEquals(listOf("scroll 0,30", "scroll 0,10"), log)
    }

    @Test fun scrollDirectionCanBeInverted() {
        val g = pad(natural = false)
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 5)
        g.move(0, 100f, 130f)
        g.move(1, 200f, 130f)
        assertEquals(listOf("scroll 0,-30"), log)
    }

    @Test fun twoFingerTapIsARightClick() {
        val g = pad()
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 10)
        g.up(1, 90); g.up(0, 100)
        assertEquals(listOf("button 1 down", "button 1 up"), log)
    }

    @Test fun threeFingerTapIsAMiddleClick() {
        val g = pad()
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 5); g.down(2, 300f, 100f, 10)
        g.up(2, 80); g.up(1, 85); g.up(0, 90)
        assertEquals(listOf("button 2 down", "button 2 up"), log)
    }

    @Test fun theFingerLeftAfterAScrollDoesNotMoveThePointer() {
        val g = pad()
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 5)
        g.move(0, 100f, 140f)
        g.move(1, 200f, 140f)
        log.clear()
        g.up(1, 40)
        g.move(0, 150f, 200f)
        g.up(0, 80)
        assertTrue(log.isEmpty())
    }

    @Test fun cancelReleasesAHeldDrag() {
        val g = pad()
        g.down(0, 0f, 0f, 0); g.up(0, 50)
        g.down(0, 0f, 0f, 100); g.move(0, 40f, 0f)
        log.clear()
        g.cancel()
        assertEquals(listOf("button 0 up"), log)
    }

    @Test fun aSecondFingerDuringADragLetsGo() {
        val g = pad()
        g.down(0, 0f, 0f, 0); g.up(0, 50)
        g.down(0, 0f, 0f, 100); g.move(0, 40f, 0f)
        log.clear()
        g.down(1, 100f, 0f, 130)
        assertEquals(listOf("button 0 up"), log)
    }

    @Test fun movingTheFingersApartIsATransformThatZoomsIn() {
        val g = pad()
        g.down(0, 400f, 300f, 0); g.down(1, 500f, 300f, 5)
        g.move(0, 380f, 300f)
        g.move(1, 520f, 300f)          // both out: 100 px apart became 140, sent in one go
        g.move(1, 546f, 300f)
        assertEquals(listOf("transform 1.40 0", "transform 1.19 0"), log)
        log.clear()
        g.move(1, 510f, 300f)          // back in: below 1
        assertEquals(listOf("transform 0.78 0"), log)
        log.clear()
        g.up(1, 90)
        assertEquals(listOf("transform end"), log)
    }

    @Test fun turningTheFingersAroundEachOtherIsATransformThatRotates() {
        val g = pad()
        g.down(0, 400f, 300f, 0); g.down(1, 500f, 300f, 5)
        // Left finger up, right finger down: they swing around their midpoint.
        g.move(0, 400f, 270f)
        g.move(1, 500f, 330f)
        g.move(1, 500f, 360f)
        assertTrue(log.size == 2 && log.all { it.startsWith("transform") })
        assertTrue("clockwise on screen is positive", log.all { it.split(" ")[2].toInt() > 0 })
    }

    @Test fun aTransformNeverTurnsIntoAScrollMidGesture() {
        val g = pad()
        g.down(0, 400f, 300f, 0); g.down(1, 500f, 300f, 5)
        g.move(0, 380f, 300f)
        g.move(1, 520f, 300f)
        g.move(0, 380f, 340f)          // a sideways drift afterwards is still the same transform
        g.move(1, 520f, 340f)
        assertTrue(log.isNotEmpty() && log.none { it.startsWith("scroll") })
    }

    @Test fun aThirdFingerEndsTheTransformAndTheGestureIsNoTap() {
        val g = pad()
        g.down(0, 400f, 300f, 0); g.down(1, 500f, 300f, 5)
        g.move(0, 380f, 300f); g.move(1, 520f, 300f)
        log.clear()
        g.down(2, 450f, 400f, 60)
        assertEquals(listOf("transform end"), log)
        log.clear()
        g.up(2, 90); g.up(1, 95); g.up(0, 100)
        assertTrue(log.none { it.startsWith("button") })
    }

    @Test fun threeFingersThatWanderALittleStillTap() {
        // Fingers that land together shift by a few millimetres; with a tight pixel slop that read as
        // a scroll and the tap never fired. Here every finger drifts 30 px against a 40 px allowance.
        val g = pad().also { it.slopPx = 12f; it.multiSlopPx = 40f }
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 20); g.down(2, 300f, 100f, 40)
        g.move(0, 125f, 120f); g.move(1, 200f, 130f); g.move(2, 280f, 100f)
        g.up(2, 150); g.up(1, 170); g.up(0, 190)
        assertEquals(listOf("button 2 down", "button 2 up"), log)
    }

    @Test fun threeFingersThatReallyMoveDoNotTap() {
        val g = pad().also { it.multiSlopPx = 40f }
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 20); g.down(2, 300f, 100f, 40)
        g.move(1, 200f, 190f)
        g.up(2, 150); g.up(1, 170); g.up(0, 190)
        assertTrue(log.isEmpty())
    }

    @Test fun aMultiFingerTapMayTakeLongerThanASingleFingerTap() {
        val g = pad()
        for (i in 0 until 3) g.down(i, 100f + 100 * i, 100f, 30L * i)
        for (i in 2 downTo 0) g.up(i, 330L + 20L * (2 - i))
        assertEquals(listOf("button 2 down", "button 2 up"), log)
        log.clear()
        g.down(0, 100f, 100f, 1000); g.up(0, 1330)
        assertTrue("a slow single tap is still no click", log.isEmpty())
    }

    @Test fun fourAndFiveFingerTapsSwitchWorkspace() {
        val g = pad()
        for (i in 0 until 4) g.down(i, 100f + 100 * i, 100f, i.toLong())
        for (i in 3 downTo 0) g.up(i, 60L)
        assertEquals(listOf("workspace -1"), log)
        log.clear()
        for (i in 0 until 5) g.down(i, 100f + 100 * i, 100f, 200L + i)
        for (i in 4 downTo 0) g.up(i, 260L)
        assertEquals(listOf("workspace 1"), log)
    }

    @Test fun inTheZoneMethodATapInTheBottomRightIsARightClick() {
        val g = pad(zone = true)
        g.down(0, 800f, 570f, 0); g.up(0, 60)
        assertEquals(listOf("button 1 down", "button 1 up"), log)
    }

    @Test fun theZoneFollowsTheRectangleNotTheScreen() {
        // A centred 800x500 area on a 1000x600 surface: its strip is y 425..550, right half x 500..900.
        val g = pad(zone = true).also { it.setBounds(100f, 50f, 900f, 550f) }
        g.down(0, 800f, 540f, 0); g.up(0, 60)          // inside the area's bottom-right
        g.down(0, 800f, 400f, 400); g.up(0, 460)       // right half, above the strip
        assertEquals(
            listOf("button 1 down", "button 1 up", "button 0 down", "button 0 up"),
            log
        )
    }

    @Test fun inTheZoneMethodTheRestOfTheSurfaceStillLeftClicks() {
        val g = pad(zone = true)
        g.down(0, 200f, 570f, 0); g.up(0, 60)          // bottom left
        g.down(0, 800f, 100f, 400); g.up(0, 460)       // right, but above the strip
        assertEquals(List(2) { listOf("button 0 down", "button 0 up") }.flatten(), log)
    }

    @Test fun inTheZoneMethodAFingerSlidingOutOfTheZoneStillMovesThePointer() {
        val g = pad(zone = true)
        g.down(0, 800f, 570f, 0)
        g.move(0, 800f, 500f)
        g.up(0, 80)
        assertEquals(listOf("move 0,-70"), log)
    }

    @Test fun inTheZoneMethodATwoFingerTapDoesNothing() {
        val g = pad(zone = true)
        g.down(0, 100f, 100f, 0); g.down(1, 200f, 100f, 10)
        g.up(1, 90); g.up(0, 100)
        assertTrue(log.isEmpty())
    }
}
