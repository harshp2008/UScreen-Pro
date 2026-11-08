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

    /** Left, top, right, bottom of the touch area, in the pixels of the surface it sits on. */
    data class Bounds(val left: Float, val top: Float, val right: Float, val bottom: Float) {
        val width get() = right - left
        val height get() = bottom - top

        fun contains(x: Float, y: Float) = x >= left && x <= right && y >= top && y <= bottom

        /** The point of this rectangle nearest to ([x], [y]). */
        fun clampX(x: Float) = x.coerceIn(left, right)
        fun clampY(y: Float) = y.coerceIn(top, bottom)
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
