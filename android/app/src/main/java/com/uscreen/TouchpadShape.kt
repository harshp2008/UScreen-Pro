package com.uscreen

/**
 * The part of the screen that acts as the touchpad. `full_screen` is all of it; the two rectangles
 * are the largest centred 4:3 or 16:9 rectangle that fits, like the pad of a laptop. Touches outside
 * it are ignored, and the tablet app draws it so the user can see where to touch.
 */
object TouchpadShape {
    const val FULL = "full_screen"
    const val STANDARD = "standard_rectangle"
    const val WIDE = "wide_rectangle"
    val SHAPES = listOf(FULL, STANDARD, WIDE)

    /** Height of the right-click strip along the bottom edge, as a fraction of the touch area. */
    const val ZONE_FRACTION = 0.15f

    /** Height of the button strip of the Classic style, as a fraction of the touch area. */
    const val CLASSIC_STRIP = 0.2f

    /** Left, top, right, bottom of the touch area, in the pixels of the surface it sits on. */
    data class Bounds(val left: Float, val top: Float, val right: Float, val bottom: Float) {
        val width get() = right - left
        val height get() = bottom - top

        /** Whether ([x], [y]) is inside the rectangle, edges included. */
        fun contains(x: Float, y: Float) = x >= left && x <= right && y >= top && y <= bottom

        /** The point of this rectangle nearest to ([x], [y]). */
        fun clampX(x: Float) = x.coerceIn(left, right)
        /** The y value nearest to [y] inside the rectangle. */
        fun clampY(y: Float) = y.coerceIn(top, bottom)
    }

    /** The part of [area] above the Classic button strip: where fingers move the pointer. */
    fun classicPad(area: Bounds) = Bounds(area.left, area.top, area.right, area.bottom - area.height * CLASSIC_STRIP)

    /** The Classic button strip along the bottom of [area]. */
    fun classicStrip(area: Bounds) = Bounds(area.left, area.bottom - area.height * CLASSIC_STRIP, area.right, area.bottom)

    /**
     * The button under horizontal position [x] of [strip]: 0 left, 1 right, 2 middle. Two buttons split the strip in half;
     * with a [middle] button the outer ones take 40 % each and the middle one 20 %.
     */
    fun classicButton(x: Float, strip: Bounds, middle: Boolean): Int {
        val f = if (strip.width <= 0f) 0f else (x - strip.left) / strip.width
        return if (!middle) {
            if (f < 0.5f) 0 else 1
        } else {
            when {
                f < 0.4f -> 0
                f < 0.6f -> 2
                else -> 1
            }
        }
    }

    /** Width over height of a rectangle shape, or null for the full screen. */
    fun aspect(shape: String): Float? = when (shape) {
        STANDARD -> 4f / 3f
        WIDE -> 16f / 9f
        else -> null
    }

    /**
     * The touch area for [shape] on a surface of [width] x [height]: centred, as large as the aspect
     * ratio allows. A surface already narrower than the ratio keeps its full width and loses height.
     */
    fun bounds(shape: String, width: Float, height: Float): Bounds {
        val aspect = aspect(shape) ?: return Bounds(0f, 0f, width, height)
        if (width <= 0f || height <= 0f) return Bounds(0f, 0f, width, height)
        val w = minOf(width, height * aspect)
        val h = w / aspect
        val l = (width - w) / 2f
        val t = (height - h) / 2f
        return Bounds(l, t, l + w, t + h)
    }
}
