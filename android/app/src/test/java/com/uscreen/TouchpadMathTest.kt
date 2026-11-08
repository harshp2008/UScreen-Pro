package com.uscreen

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TouchpadMathTest {
    @Test fun slowMovementIsNotLostToRounding() {
        // 0.4 px per event: 100 events must still move the pointer 40 px, not 0.
        val sub = SubPixel()
        var x = 0
        repeat(100) { sub.add(0.4f, 0f)?.let { x += it.first } }
        assertEquals(40, x)
    }

    @Test fun theRemainderKeepsItsSignForNegativeMovement() {
        val sub = SubPixel()
        var y = 0
        repeat(50) { sub.add(0f, -0.6f)?.let { y += it.second } }
        assertEquals(-30, y)
    }

    @Test fun nothingIsReportedUntilAWholeUnitIsDue() {
        val sub = SubPixel()
        assertNull(sub.add(0.3f, 0.3f))
        assertNull(sub.add(0.3f, 0.3f))
        assertEquals(1 to 1, sub.add(0.5f, 0.5f))
    }

    @Test fun resetForgetsTheRemainder() {
        val sub = SubPixel()
        sub.add(0.9f, 0f)
        sub.reset()
        assertNull(sub.add(0.2f, 0f))
    }

    @Test fun gentleMovementIsOneToOneAndFastMovementIsBoosted() {
        assertEquals(3.2f, PointerSpeed.apply(2f, 1.6f), 0.001f)
        // 5 px * 1.6 = 8 px is above the 6 px threshold: boosted by 1.25.
        assertEquals(10f, PointerSpeed.apply(5f, 1.6f), 0.001f)
        assertEquals(-10f, PointerSpeed.apply(-5f, 1.6f), 0.001f)
    }

    @Test fun aTapIsShortAndBarelyMoved() {
        assertTrue(TapRules.isTap(120, 4f, TapRules.TAP_MAX_MS, 20f))
        assertFalse("too slow", TapRules.isTap(400, 4f, TapRules.TAP_MAX_MS, 20f))
        assertFalse("moved too far", TapRules.isTap(120, 40f, TapRules.TAP_MAX_MS, 20f))
        assertTrue("a multi-finger tap may take longer", TapRules.isTap(320, 4f, TapRules.MULTI_TAP_MAX_MS, 20f))
    }

    @Test fun multiFingerTapsDoWhatTheyAreMeantTo() {
        assertEquals(TapRules.Multi.NONE, TapRules.multiAction(2, twoFingerRightClick = false))
        assertEquals(TapRules.Multi.RIGHT_CLICK, TapRules.multiAction(2, twoFingerRightClick = true))
        assertEquals(TapRules.Multi.MIDDLE_CLICK, TapRules.multiAction(3, false))
        assertEquals(TapRules.Multi.WORKSPACE_PREV, TapRules.multiAction(4, false))
        assertEquals(TapRules.Multi.WORKSPACE_NEXT, TapRules.multiAction(5, false))
        assertEquals(TapRules.Multi.NONE, TapRules.multiAction(1, false))
    }

    @Test fun aScrollIsRecognisedAndTheMovementSoFarIsReleased() {
        val c = TwoFingerClassifier(decidePx = 20f)
        assertNull(c.feed(0f, 6f, 0f))
        assertNull(c.feed(0f, 6f, 1f))
        val m = c.feed(0f, 10f, 0f)
        assertNotNull(m)
        assertEquals(TwoFingerClassifier.Kind.SCROLL, c.kind)
        assertEquals(22f, m!!.scrollY, 0.001f) // 6 + 6 + 10: nothing lost to the waiting
        assertEquals(0f, m.pinch, 0.001f)
        // From here on only scroll is reported, even if the gap changes a little.
        assertEquals(TwoFingerClassifier.Move(0f, 4f, 0f), c.feed(0f, 4f, 3f))
    }

    @Test fun aPinchIsRecognisedWhenTheGapChangesMoreThanTheCentreMoves() {
        val c = TwoFingerClassifier(decidePx = 20f)
        assertNull(c.feed(1f, 0f, 8f))
        val m = c.feed(1f, 0f, 16f)
        assertEquals(TwoFingerClassifier.Kind.PINCH, c.kind)
        assertEquals(24f, m!!.pinch, 0.001f)
        assertEquals(0f, m.scrollX, 0.001f)
        assertEquals(TwoFingerClassifier.Move(0f, 0f, 5f), c.feed(30f, 30f, 5f))
    }

    @Test fun aPinchMustClearTheScrollBias() {
        // Gap change 21, centre travel 20: only 5% more, so it stays a scroll.
        val c = TwoFingerClassifier(decidePx = 20f)
        c.feed(0f, 20f, 21f)
        assertEquals(TwoFingerClassifier.Kind.SCROLL, c.kind)
    }
}
