/*
 * Tests of [PenTilt]: the signs and magnitudes of the tilt angles the host receives.
 */
package com.uscreen.input

import kotlin.math.PI
import org.junit.Assert.assertEquals
import org.junit.Test

/** Direction conventions of the pen tilt: x positive to the right edge, y positive towards the bottom edge, degrees. */
class PenTiltTest {
    /** A pen held upright leans nowhere. */
    @Test
    fun an_upright_pen_has_no_tilt() {
        val (x, y) = PenTilt.decompose(0.0, 1.234)
        assertEquals(0.0, x, 1e-9)
        assertEquals(0.0, y, 1e-9)
    }

    /** Orientation pi/2 points right on the screen: positive X tilt, no Y tilt. */
    @Test
    fun a_pen_leaning_right_tilts_x_positive() {
        val (x, y) = PenTilt.decompose(PI / 4, PI / 2)
        assertEquals(45.0, x, 1e-6)
        assertEquals(0.0, y, 1e-6)
    }

    /** Orientation 0 points up on the screen, which is the negative Y direction. */
    @Test
    fun a_pen_leaning_up_tilts_y_negative() {
        val (x, y) = PenTilt.decompose(PI / 4, 0.0)
        assertEquals(0.0, x, 1e-6)
        assertEquals(-45.0, y, 1e-6)
    }
}
