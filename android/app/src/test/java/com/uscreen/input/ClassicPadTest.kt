package com.uscreen.input

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The geometry of the Classic touchpad: where the pad ends, where the buttons are, which one a finger hits. */
class ClassicPadTest {
    private val area = TouchpadShape.Bounds(0f, 0f, 1000f, 500f)

    /** The pad and the strip share the touch area and do not overlap or leave a gap. */
    @Test
    fun the_pad_and_the_strip_split_the_area() {
        val pad = TouchpadShape.classicPad(area)
        val strip = TouchpadShape.classicStrip(area)
        assertEquals(0f, pad.top, 0f)
        assertEquals(strip.top, pad.bottom, 0.001f)
        assertEquals(500f, strip.bottom, 0f)
        assertEquals(500f * TouchpadShape.CLASSIC_STRIP, strip.height, 0.001f)
        assertTrue(strip.contains(10f, 450f) && !pad.contains(10f, 450f))
    }

    /** Two buttons split the strip in half. */
    @Test
    fun two_buttons_split_the_strip_in_half() {
        val strip = TouchpadShape.classicStrip(area)
        assertEquals(0, TouchpadShape.classicButton(100f, strip, false))
        assertEquals(0, TouchpadShape.classicButton(499f, strip, false))
        assertEquals(1, TouchpadShape.classicButton(501f, strip, false))
        assertEquals(1, TouchpadShape.classicButton(900f, strip, false))
    }

    /** With a middle button the outer two take 40 % each and the middle 20 %. */
    @Test
    fun a_middle_button_takes_the_centre_fifth() {
        val strip = TouchpadShape.classicStrip(area)
        assertEquals(0, TouchpadShape.classicButton(399f, strip, true))
        assertEquals(2, TouchpadShape.classicButton(401f, strip, true))
        assertEquals(2, TouchpadShape.classicButton(599f, strip, true))
        assertEquals(1, TouchpadShape.classicButton(601f, strip, true))
    }
}
