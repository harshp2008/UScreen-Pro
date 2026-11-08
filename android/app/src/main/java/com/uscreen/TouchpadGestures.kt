package com.uscreen

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

/**
 * Turns finger contacts into what a laptop touchpad does: move, click, drag,
 * right-click and scroll. It knows nothing about Android; the caller feeds
 * it contacts (an id, a position in pixels, a time in milliseconds) and gets
 * plain mouse actions back through [Output].
 *
 *  - one finger moving: pointer movement
 *  - quick tap: left click
 *  - tap, then touch again and move: drag with the left button held; touch and
 *    lift without moving is a second click (double click)
 *  - two fingers moving the same way: scroll; moving apart or together and/or turning around each
 *    other: a transform, reported as a zoom factor and a rotation in degrees (the same two numbers
 *    Compose's `detectTransformGestures` reports) that the computer replays as a native pinch
 *  - quick two-finger tap: right click (only with the "Two-Finger Tap" right-click method); a tap
 *    in the bottom-right strip of the touch area: right click (the default "Bottom Right Zone")
 *  - three fingers tapped: middle click; four fingers: previous workspace; five: next workspace
 *
 * A finger left over after a two-finger gesture does nothing until every
 * finger is up, otherwise lifting one finger would make the pointer jump.
 *
 * The touch area is a rectangle ([setBounds]); the caller drops touches outside it.
 *
 * Ported from upstream UScreen 1.2.14 (touchpad mode, issue #40).
 */
internal class TouchpadGestures(
    private val out: Output,
    /** Pointer speed: pixels of finger travel to pixels sent to the computer. */
    var gain: Float = 1.6f,
    /** Content follows the fingers, as on a phone. */
    var naturalScroll: Boolean = true,
) {
    /** How a right click is made: a tap in the bottom-right strip (default) or a two-finger tap. */
    var rightClickZone = true

    /**
     * Travel of one finger before a touch counts as movement and not a tap. The caller sets it from
     * the panel's physical density (a few millimetres); the default suits the unit tests.
     */
    var slopPx = SLOP_PX

    /**
     * Travel of any finger, with three or more down, before a tap stops being a tap. Fingers landing
     * together shift more than one finger does, so this is wider than [slopPx].
     */
    var multiSlopPx = SLOP_PX

    /** The touch area in view pixels: left, top, right, bottom. */
    var left = 0f
        private set
    var top = 0f
        private set
    var right = 0f
        private set
    var bottom = 0f
        private set

    fun setBounds(l: Float, t: Float, r: Float, b: Float) {
        left = l
        top = t
        right = r
        bottom = b
    }

    interface Output {
        fun move(dx: Float, dy: Float)
        /** 0 = left, 1 = right, 2 = middle. */
        fun button(button: Int, down: Boolean)
        fun scroll(dx: Float, dy: Float)
        /**
         * Two fingers changed their spread by the factor [zoom] (above 1 is apart) and turned by
         * [rotationDeg] degrees (positive is clockwise on screen) since the last call.
         */
        fun transform(zoom: Float, rotationDeg: Float)
        /** The two-finger transform is over (a finger lifted, a third landed, or the touch was cancelled). */
        fun transformEnd()
        /** Four-finger tap (-1, previous workspace) or five-finger tap (+1, next workspace). */
        fun workspace(direction: Int)
    }

    /** What a two-finger gesture turned out to be; decided once, when the fingers have moved enough. */
    private enum class TwoFinger { UNDECIDED, SCROLL, TRANSFORM }

    private enum class Mode { IDLE, TAP_OR_MOVE, MOVING, DRAGGING, SCROLLING, MULTI, ABSORBING }

    private class Contact(var x: Float, var y: Float, val downX: Float, val downY: Float)

    private val contacts = LinkedHashMap<Int, Contact>()
    private var mode = Mode.IDLE
    private var startTime = 0L
    private var maxFingers = 0
    private var travelled = false
    private var lastTapUp = -1L
    private var dragCandidate = false
    private var twoFinger = TwoFinger.UNDECIDED
    private var transforming = false
    private var start0 = 0f to 0f
    private var start1 = 0f to 0f
    private var lastCx = 0f
    private var lastCy = 0f
    private var startAngle = 0f
    private var lastDist = 0f
    private var lastAngle = 0f
    private var tapInRightZone = false

    fun down(id: Int, x: Float, y: Float, time: Long) {
        if (contacts.isEmpty()) {
            startTime = time
            maxFingers = 0
            travelled = false
            dragCandidate = lastTapUp >= 0 && time - lastTapUp <= DOUBLE_TAP_MS
            mode = Mode.TAP_OR_MOVE
            tapInRightZone = rightClickZone && inRightZone(x, y)
        }
        contacts[id] = Contact(x, y, x, y)
        maxFingers = maxOf(maxFingers, contacts.size)
        if (contacts.size == 2) beginTwoFinger()
        if (contacts.size >= 2) {
            // A second finger: this is a scroll or a multi-finger tap, never a
            // drag, and a pointer already in motion stops where it is.
            dragCandidate = false
            if (mode == Mode.DRAGGING) {
                out.button(0, false)
                mode = Mode.ABSORBING
            } else if (mode != Mode.ABSORBING) {
                mode = Mode.SCROLLING
            }
        }
        if (contacts.size >= 3) {
            // Three or more fingers are taps, never a scroll or a transform: the two-finger gesture
            // that may have been under way ends, and from here only "did anything travel" matters.
            endTransform()
            if (mode != Mode.ABSORBING) mode = Mode.MULTI
        }
    }

    fun move(id: Int, x: Float, y: Float) {
        val c = contacts[id] ?: return
        val dx = x - c.x
        val dy = y - c.y
        c.x = x
        c.y = y
        when (mode) {
            Mode.TAP_OR_MOVE -> {
                if (hypot(x - c.downX, y - c.downY) > slopPx) {
                    travelled = true
                    // Everything since the finger landed, so nothing is lost
                    // to the dead zone.
                    val ox = x - c.downX
                    val oy = y - c.downY
                    if (dragCandidate) {
                        out.button(0, true)
                        mode = Mode.DRAGGING
                    } else {
                        mode = Mode.MOVING
                    }
                    out.move(ox * gain, oy * gain)
                }
            }
            Mode.MOVING, Mode.DRAGGING -> out.move(dx * gain, dy * gain)
            Mode.SCROLLING -> if (contacts.size == 2) twoFingerMove()
            Mode.MULTI -> {
                if (hypot(x - c.downX, y - c.downY) > multiSlopPx) travelled = true
            }
            else -> {}
        }
    }

    fun up(id: Int, time: Long) {
        if (contacts.remove(id) == null) return
        if (contacts.isNotEmpty()) {
            endTransform()
            if (mode == Mode.SCROLLING || mode == Mode.MOVING) mode = Mode.ABSORBING
            return
        }

        val quick = time - startTime <= (if (maxFingers >= 3) MULTI_TAP_MS else TAP_MS)
        when (mode) {
            Mode.TAP_OR_MOVE, Mode.SCROLLING, Mode.MULTI, Mode.ABSORBING -> {
                if (!travelled && quick) {
                    when (maxFingers) {
                        1 -> if (tapInRightZone) {
                            click(1)
                        } else {
                            click(0)
                            lastTapUp = time
                        }
                        2 -> if (!rightClickZone) click(1)
                        3 -> click(2)
                        4 -> out.workspace(-1)
                        5 -> out.workspace(1)
                    }
                    if (maxFingers != 1) lastTapUp = -1
                } else {
                    lastTapUp = -1
                }
            }
            Mode.DRAGGING -> {
                out.button(0, false)
                lastTapUp = -1
            }
            else -> lastTapUp = -1
        }
        mode = Mode.IDLE
    }

    /** The system took the touch away (a gesture, a dialog, the pen). */
    fun cancel() {
        if (mode == Mode.DRAGGING) out.button(0, false)
        endTransform()
        contacts.clear()
        mode = Mode.IDLE
        lastTapUp = -1
    }

    /** The bottom [ZONE_FRACTION] of the touch area, right half: a tap here is a right click. */
    private fun inRightZone(x: Float, y: Float): Boolean =
        right > left && bottom > top &&
            y >= bottom - (bottom - top) * ZONE_FRACTION && x >= (left + right) / 2f

    private fun twoContacts(): Pair<Contact, Contact> {
        val it = contacts.values.iterator()
        return it.next() to it.next()
    }

    private fun beginTwoFinger() {
        val (a, b) = twoContacts()
        twoFinger = TwoFinger.UNDECIDED
        start0 = a.x to a.y
        start1 = b.x to b.y
        startAngle = atan2(b.y - a.y, b.x - a.x)
        lastDist = hypot(b.x - a.x, b.y - a.y)
        lastAngle = startAngle
        lastCx = (a.x + b.x) / 2f
        lastCy = (a.y + b.y) / 2f
    }

    private fun endTransform() {
        if (transforming) {
            transforming = false
            out.transformEnd()
        }
    }

    /**
     * Two fingers moved. Once they have travelled past the slop the gesture is classified from what
     * each finger did since they landed: moving together is a scroll; moving along the line between
     * them (pinch) or across it (rotate) is a transform. The choice then holds until the fingers
     * lift, and what was travelled before it was made is sent with the first message.
     *
     * A transform reports the spread factor and the turn together, the way a touchpad's pinch does,
     * so a pinch that also turns a little zooms and rotates.
     *
     * Android reports the fingers one at a time here, so a lone moving finger looks like both a
     * scroll and a turn. The decision waits until both fingers have moved, or one has gone far
     * enough that it can only be a transform.
     */
    private fun twoFingerMove() {
        val (a, b) = twoContacts()
        val dist = hypot(b.x - a.x, b.y - a.y)
        val angle = atan2(b.y - a.y, b.x - a.x)
        val cx = (a.x + b.x) / 2f
        val cy = (a.y + b.y) / 2f
        if (twoFinger == TwoFinger.UNDECIDED) {
            val d0x = a.x - start0.first
            val d0y = a.y - start0.second
            val d1x = b.x - start1.first
            val d1y = b.y - start1.second
            val ux = cos(startAngle)
            val uy = sin(startAngle)
            val scroll = hypot(d0x + d1x, d0y + d1y) / 2f
            val pinch = abs((d1x - d0x) * ux + (d1y - d0y) * uy) / 2f
            val rotate = abs(-(d1x - d0x) * uy + (d1y - d0y) * ux) / 2f
            val best = maxOf(scroll, pinch, rotate)
            if (best <= slopPx) return
            val bothMoved = hypot(d0x, d0y) > slopPx / 2f && hypot(d1x, d1y) > slopPx / 2f
            if (!bothMoved && best < 3 * slopPx) return
            twoFinger = if (bothMoved && scroll >= pinch && scroll >= rotate) TwoFinger.SCROLL else TwoFinger.TRANSFORM
            travelled = true
        }
        when (twoFinger) {
            TwoFinger.SCROLL -> {
                val sign = if (naturalScroll) 1f else -1f
                out.scroll((cx - lastCx) * sign, (cy - lastCy) * sign)
                lastCx = cx
                lastCy = cy
            }
            TwoFinger.TRANSFORM -> {
                val zoom = if (lastDist > 1f) dist / lastDist else 1f
                val turn = Math.toDegrees(wrap(angle - lastAngle).toDouble()).toFloat()
                transforming = true
                out.transform(zoom, turn)
                lastDist = dist
                lastAngle = angle
            }
            TwoFinger.UNDECIDED -> {}
        }
    }

    /** An angle difference folded into -π..π, so crossing the ±π seam is a small turn. */
    private fun wrap(a: Float): Float {
        var r = a
        while (r > PI.toFloat()) r -= 2f * PI.toFloat()
        while (r < -PI.toFloat()) r += 2f * PI.toFloat()
        return r
    }

    private fun click(button: Int) {
        out.button(button, true)
        out.button(button, false)
    }

    companion object {
        /** Default travel before a touch counts as movement and not a tap. */
        const val SLOP_PX = 12f
        const val TAP_MS = 250L

        /** Fingers land and lift one after another, so a multi-finger tap gets longer. */
        const val MULTI_TAP_MS = 400L
        const val DOUBLE_TAP_MS = 300L

        /** Height of the right-click strip along the bottom edge, as a fraction of the touch area. */
        const val ZONE_FRACTION = 0.15f
    }
}
