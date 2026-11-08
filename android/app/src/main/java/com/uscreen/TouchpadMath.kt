package com.uscreen

import kotlin.math.abs
import kotlin.math.hypot

/*
 * The arithmetic of the touchpad, kept free of Android types so the JVM tests cover it. The engine
 * in TouchpadEngine.kt feeds these with real MotionEvents.
 *
 * Where the ideas come from (studied, not copied; see experiments_and_findings/72_input_engine_rewrite.md):
 *  - Winlator's TouchpadView: up to four fingers tracked by pointer id with their start position and
 *    time, a tap is "short and barely moved", a multi-finger tap is judged when the first finger
 *    lifts, and the pointer gets a small boost above a speed threshold.
 *  - Axon: telling a two-finger scroll from a pinch by which one moved more, with a bias to scroll.
 *  - PhonePad: keeping the fractional part of every pointer move so slow drags do not stall.
 */

/**
 * Keeps the fraction of a movement that does not make a whole unit. A finger crawling at 0.4 px per
 * event would otherwise round to nothing every time and the pointer would never move; with the
 * remainder carried over it moves one unit every 2.5 events, at exactly the finger's speed.
 */
class SubPixel {
    private var restX = 0f
    private var restY = 0f

    /**
     * Add a movement. Returns the whole units (dx, dy) that are now due, or null when neither axis
     * has reached one yet.
     */
    fun add(dx: Float, dy: Float): Pair<Int, Int>? {
        val fx = restX + dx
        val fy = restY + dy
        // Truncation toward zero, so the remainder keeps the sign of the movement.
        val ix = fx.toInt()
        val iy = fy.toInt()
        restX = fx - ix
        restY = fy - iy
        return if (ix == 0 && iy == 0) null else ix to iy
    }

    /** Forget the remainder: a new gesture starts from a clean slate. */
    fun reset() {
        restX = 0f
        restY = 0f
    }
}

/** The pointer's speed curve: gentle movement is 1:1 with the sensitivity, fast movement gets a boost. */
object PointerSpeed {
    /** Per-event travel in pixels above which the boost applies (Winlator's threshold). */
    const val THRESHOLD_PX = 6f

    /** The boost (Winlator's `CURSOR_ACCELERATION`). */
    const val FACTOR = 1.25f

    /** One axis of a movement of [delta] pixels, scaled by [sensitivity] and boosted when fast. */
    fun apply(delta: Float, sensitivity: Float): Float {
        val v = delta * sensitivity
        return if (abs(v) > THRESHOLD_PX) v * FACTOR else v
    }
}

/** Rules for what counts as a tap and what a multi-finger tap does. */
object TapRules {
    /** A single finger tap: down and up within this long, the finger barely moved. */
    const val TAP_MAX_MS = 250L

    /** Multi-finger taps get longer: the fingers land and lift one after another. */
    const val MULTI_TAP_MAX_MS = 350L

    /** What lifting several fingers together does. */
    enum class Multi { NONE, RIGHT_CLICK, MIDDLE_CLICK, WORKSPACE_PREV, WORKSPACE_NEXT }

    /**
     * The action of a tap with [count] fingers. Two fingers right-click only with the "two-finger
     * tap" method; with the bottom-right-zone method a two-finger tap does nothing.
     */
    fun multiAction(count: Int, twoFingerRightClick: Boolean): Multi = when (count) {
        2 -> if (twoFingerRightClick) Multi.RIGHT_CLICK else Multi.NONE
        3 -> Multi.MIDDLE_CLICK
        4 -> Multi.WORKSPACE_PREV
        5 -> Multi.WORKSPACE_NEXT
        else -> Multi.NONE
    }

    /** True when a finger down for [durationMs] that travelled [travelPx] still counts as a tap. */
    fun isTap(durationMs: Long, travelPx: Float, maxMs: Long, maxTravelPx: Float): Boolean =
        durationMs in 0..maxMs && travelPx <= maxTravelPx
}

/**
 * Decides, once per two-finger gesture, whether it is a scroll or a pinch, from how far the centre
 * of the two fingers travelled against how much the gap between them changed. Nothing is reported
 * until there is enough movement to tell; then the movement so far is released at once, so no
 * distance is lost to the waiting, and after that only the chosen kind is reported.
 *
 * A pinch must beat the scroll travel by [SCROLL_BIAS] to win, because an accidental zoom is much
 * more annoying than an accidental scroll.
 *
 * @param decidePx movement (of either kind) after which the gesture is classified.
 */
class TwoFingerClassifier(private val decidePx: Float) {
    enum class Kind { UNDECIDED, SCROLL, PINCH }

    /** What to send now: scroll distance in pixels, or the change of the gap between the fingers. */
    data class Move(val scrollX: Float, val scrollY: Float, val pinch: Float)

    var kind = Kind.UNDECIDED
        private set

    private var sx = 0f
    private var sy = 0f
    private var span = 0f
    private var scrollTravel = 0f
    private var pinchTravel = 0f

    /**
     * Feed one step: the centre moved by ([dx], [dy]) and the gap between the fingers changed by
     * [dSpan]. Returns what to emit, or null while undecided (or when there is nothing to emit).
     */
    fun feed(dx: Float, dy: Float, dSpan: Float): Move? = when (kind) {
        Kind.UNDECIDED -> {
            sx += dx
            sy += dy
            span += dSpan
            scrollTravel += hypot(dx, dy)
            pinchTravel += abs(dSpan)
            if (maxOf(scrollTravel, pinchTravel) < decidePx) {
                null
            } else {
                kind = if (pinchTravel > SCROLL_BIAS * scrollTravel) Kind.PINCH else Kind.SCROLL
                if (kind == Kind.PINCH) Move(0f, 0f, span) else Move(sx, sy, 0f)
            }
        }
        Kind.SCROLL -> if (dx == 0f && dy == 0f) null else Move(dx, dy, 0f)
        Kind.PINCH -> if (dSpan == 0f) null else Move(0f, 0f, dSpan)
    }

    companion object {
        const val SCROLL_BIAS = 1.1f
    }
}
