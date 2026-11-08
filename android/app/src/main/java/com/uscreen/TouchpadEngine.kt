package com.uscreen

import android.content.Context
import android.content.res.Configuration
import android.os.Handler
import android.os.Looper
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import kotlin.math.hypot

/**
 * Turns the fingers on the tablet's touchpad area into mouse actions for the computer.
 *
 * It is the tablet half of Touchpad mode: it gets every finger [MotionEvent] through
 * [onTouchEvent] and reports plain actions through [Output] (pointer movement in whole pixels,
 * scroll, buttons, a pinch distance, a workspace switch). The computer only injects them.
 *
 * ### How it tracks fingers
 * Like Winlator's `TouchpadView`, a [Finger] is kept per Android pointer id (at most
 * [MAX_FINGERS]) with the place and time it landed, so "how long was it down" and "how far did it
 * travel" are answered per finger, not for the gesture as a whole. A finger that lands outside the
 * touch area (or that the panel flags as a palm) is remembered in [outside] and ignored for its
 * whole life: nothing is sent and no gesture sees it. A finger that slides out of the area stops at
 * the edge.
 *
 * ### What does what
 * - **One finger moving**: pointer movement. The first [slopPx] of travel is a dead zone (so a tap
 *   never nudges the pointer); then everything since touch-down is sent, and from there each step.
 *   Steps are scaled by [sensitivity], boosted when fast ([PointerSpeed]) and the fractional part is
 *   carried over ([SubPixel]) so slow movement does not stall.
 * - **Tap** (Android's [GestureDetector]): left click, or right click in the bottom-right
 *   [TouchpadShape.ZONE_FRACTION] of the area when [rightClickZone] is on.
 * - **Tap, then touch again and move** (the detector's double-tap events): drag with the left
 *   button held; tap-tap without moving is a second click.
 * - **Two fingers**: a scroll or a pinch, decided once per gesture ([TwoFingerClassifier]). The
 *   pinch distance comes from Android's [ScaleGestureDetector], which also filters out noise.
 * - **Several fingers tapped** (judged when the first one lifts, like Winlator): three is a middle
 *   click, four the previous workspace, five the next; two is a right click only when
 *   [rightClickZone] is off ([TapRules]). Tolerances are physical ([setPanelDensity]) because a
 *   fixed pixel count was far too tight on a dense panel.
 *
 * Once a second finger has landed, a finger left over after the others lift does nothing until all
 * are up, otherwise lifting one finger would make the pointer jump.
 *
 * Clicks are a press, 30 ms, a release: a press and release in the same instant is easily lost by
 * the applications on the other end (Winlator spaces them the same way).
 */
internal class TouchpadEngine(context: Context, private val out: Output) {
    /** What the engine asks the computer to do. Movement and scroll are in whole pixels. */
    interface Output {
        fun move(dx: Int, dy: Int)

        fun scroll(dx: Int, dy: Int)

        /** 0 = left, 1 = right, 2 = middle. */
        fun button(button: Int, down: Boolean)

        /** Two fingers moved apart (positive) or together (negative) by this many pixels. */
        fun pinch(amount: Float)

        /** The pinch is over (a finger lifted or the touch was cancelled). */
        fun pinchEnd()

        /** Four-finger tap (-1, previous workspace) or five-finger tap (+1, next workspace). */
        fun workspace(direction: Int)

        /** A finger landed inside the touch area after none were down: the user is using the pad. */
        fun touched()
    }

    /** Pointer speed: pixels of finger travel to pixels sent to the computer. */
    var sensitivity = 1.6f

    /** Content follows the fingers, as on a phone. */
    var naturalScroll = true

    /** A tap in the bottom-right strip right-clicks (and a two-finger tap does not); off: the other way round. */
    var rightClickZone = true

    /** Travel of one finger before it is a movement and not a tap; set from the panel density. */
    var slopPx = 12f
        private set

    /** Travel of any finger, with several down, before a multi-finger tap stops being a tap. */
    var multiSlopPx = 30f
        private set

    /** Pixels per millimetre of the panel, for the tolerances above. */
    fun setPanelDensity(pixelsPerMm: Float) {
        if (pixelsPerMm <= 0f) return
        slopPx = SLOP_MM * pixelsPerMm
        multiSlopPx = MULTI_SLOP_MM * pixelsPerMm
    }

    private var left = 0f
    private var top = 0f
    private var right = 0f
    private var bottom = 0f

    /** The touch area in view pixels. Touches outside it are ignored. */
    fun setBounds(l: Float, t: Float, r: Float, b: Float) {
        left = l
        top = t
        right = r
        bottom = b
    }

    /** One finger on the pad. */
    private class Finger(val id: Int, x: Float, y: Float, val downTime: Long) {
        val startX = x
        val startY = y
        var x = x
        var y = y
        var lastX = x
        var lastY = y

        fun moveTo(nx: Float, ny: Float) {
            lastX = x
            lastY = y
            x = nx
            y = ny
        }

        fun travel() = hypot(x - startX, y - startY)
    }

    private val fingers = LinkedHashMap<Int, Finger>()

    /** Pointer ids ignored for their whole life: outside the area, palms, or past [MAX_FINGERS]. */
    private val outside = HashSet<Int>()

    private val sub = SubPixel()
    private val scrollSub = SubPixel()

    /** The pointer has started moving in this gesture (the dead zone is behind it). */
    private var moving = false

    /** A second finger has landed in this gesture: no more taps or single-finger movement. */
    private var absorb = false

    /** The multi-finger tap of this gesture has been judged (at the first lift). */
    private var multiJudged = false

    private var classifier = TwoFingerClassifier(30f)
    private var lastCx = 0f
    private var lastCy = 0f
    private var pendingSpan = 0f

    /** A pinch is in progress, so the computer is holding Ctrl and must be told when it ends. */
    private var pinching = false

    /** Left button held for a drag. */
    private var dragging = false
    private var doubleTapPending = false
    private var doubleTapX = 0f
    private var doubleTapY = 0f

    /** Detectors are not fed while a finger outside the area is down; they were told the touch ended. */
    private var detectorsMuted = false

    private val handler = Handler(Looper.getMainLooper())
    private val pendingReleases = LinkedHashMap<Runnable, Int>()

    /**
     * Taps and double taps. The detector is given a context whose density is 1.6 times the real
     * one, which scales its touch slop and double-tap slop up the same way: its defaults are tuned
     * for a phone and a fingertip on a tablet wanders further.
     */
    private val tapDetector = GestureDetector(
        context.createConfigurationContext(
            Configuration(context.resources.configuration).apply { densityDpi = (densityDpi * 1.6f).toInt() }
        ),
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onSingleTapUp(e: MotionEvent): Boolean {
                if (absorb || moving || e.eventTime - e.downTime > TapRules.TAP_MAX_MS) return false
                click(if (rightClickZone && inRightZone(e.x, e.y)) RIGHT else LEFT)
                return true
            }

            override fun onDoubleTapEvent(e: MotionEvent): Boolean {
                when (e.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        doubleTapPending = true
                        doubleTapX = e.x
                        doubleTapY = e.y
                    }
                    MotionEvent.ACTION_MOVE ->
                        if (doubleTapPending && !dragging && !absorb &&
                            hypot(e.x - doubleTapX, e.y - doubleTapY) > slopPx
                        ) {
                            dragging = true
                            out.button(LEFT, true)
                        }
                    MotionEvent.ACTION_UP -> {
                        if (dragging) {
                            dragging = false
                            out.button(LEFT, false)
                        } else if (doubleTapPending && !absorb) {
                            click(LEFT)
                        }
                        doubleTapPending = false
                    }
                }
                return true
            }
        }
    ).also { it.setIsLongpressEnabled(false) }

    /** Pinch distance, with the detector's own noise filter. */
    private val scaleDetector = ScaleGestureDetector(
        context,
        object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            override fun onScale(detector: ScaleGestureDetector): Boolean {
                pendingSpan += detector.currentSpan - detector.previousSpan
                return true
            }
        }
    ).also { it.isQuickScaleEnabled = false }

    /** Feed one touch event for the finger pointers of the pad. */
    fun onTouchEvent(event: MotionEvent) {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                down(event)
                feedDetectors(event)
            }
            MotionEvent.ACTION_MOVE -> {
                feedDetectors(event)
                move(event)
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                feedDetectors(event)
                up(event)
            }
            MotionEvent.ACTION_CANCEL -> cancel()
        }
    }

    private fun down(event: MotionEvent) {
        val i = event.actionIndex
        val id = event.getPointerId(i)
        val x = event.getX(i)
        val y = event.getY(i)
        if (isPalm(event, i) || !inBounds(x, y) || fingers.size >= MAX_FINGERS) {
            outside.add(id)
            return
        }
        outside.remove(id)
        if (fingers.isEmpty()) {
            newGesture()
            out.touched()
        }
        fingers[id] = Finger(id, x, y, event.eventTime)
        if (fingers.size >= 2) {
            // A second finger: no more taps or movement of the first, and a drag in progress ends.
            absorb = true
            releaseDrag()
        }
        if (fingers.size == 2) beginTwoFinger()
    }

    private fun move(event: MotionEvent) {
        for (i in 0 until event.pointerCount) {
            val f = fingers[event.getPointerId(i)] ?: continue
            f.moveTo(event.getX(i).coerceIn(left, right), event.getY(i).coerceIn(top, bottom))
        }
        when (fingers.size) {
            1 -> oneFingerMove(fingers.values.first())
            2 -> twoFingerMove()
        }
    }

    private fun oneFingerMove(f: Finger) {
        if (absorb) return
        if (!moving) {
            if (f.travel() <= slopPx) return
            moving = true
            // Everything since touch-down, so nothing is lost to the dead zone.
            emitMove(f.x - f.startX, f.y - f.startY)
        } else {
            emitMove(f.x - f.lastX, f.y - f.lastY)
        }
    }

    private fun emitMove(dx: Float, dy: Float) {
        sub.add(PointerSpeed.apply(dx, sensitivity), PointerSpeed.apply(dy, sensitivity))
            ?.let { out.move(it.first, it.second) }
    }

    private fun beginTwoFinger() {
        val (a, b) = twoFingers()
        classifier = TwoFingerClassifier(slopPx)
        lastCx = (a.x + b.x) / 2f
        lastCy = (a.y + b.y) / 2f
        pendingSpan = 0f
        scrollSub.reset()
    }

    private fun twoFingers(): Pair<Finger, Finger> {
        val it = fingers.values.iterator()
        return it.next() to it.next()
    }

    private fun twoFingerMove() {
        val (a, b) = twoFingers()
        val cx = (a.x + b.x) / 2f
        val cy = (a.y + b.y) / 2f
        val span = pendingSpan
        pendingSpan = 0f
        val m = classifier.feed(cx - lastCx, cy - lastCy, span)
        lastCx = cx
        lastCy = cy
        m ?: return
        if (m.pinch != 0f) {
            pinching = true
            out.pinch(m.pinch)
        }
        if (m.scrollX != 0f || m.scrollY != 0f) {
            val sign = if (naturalScroll) 1f else -1f
            scrollSub.add(m.scrollX * sign, m.scrollY * sign)?.let { out.scroll(it.first, it.second) }
        }
    }

    private fun up(event: MotionEvent) {
        val id = event.getPointerId(event.actionIndex)
        if (outside.remove(id)) {
            if (fingers.isEmpty() && outside.isEmpty()) detectorsMuted = false
            return
        }
        val f = fingers[id] ?: return
        endPinch()
        if (fingers.size >= 2 && !multiJudged) judgeMultiTap(event.eventTime)
        fingers.remove(f.id)
        if (fingers.isEmpty()) {
            releaseDrag()
            resetGesture()
            if (outside.isEmpty()) detectorsMuted = false
        }
    }

    /**
     * Called when the first of several fingers lifts: if every finger is still a tap (short, barely
     * moved) and the two-finger gesture never turned into a scroll or pinch, do the multi-finger action.
     */
    private fun judgeMultiTap(now: Long) {
        multiJudged = true
        if (classifier.kind != TwoFingerClassifier.Kind.UNDECIDED && fingers.size == 2) return
        val allTaps = fingers.values.all {
            TapRules.isTap(now - it.downTime, it.travel(), TapRules.MULTI_TAP_MAX_MS, multiSlopPx)
        }
        if (!allTaps) return
        when (TapRules.multiAction(fingers.size, twoFingerRightClick = !rightClickZone)) {
            TapRules.Multi.RIGHT_CLICK -> click(RIGHT)
            TapRules.Multi.MIDDLE_CLICK -> click(MIDDLE)
            TapRules.Multi.WORKSPACE_PREV -> out.workspace(-1)
            TapRules.Multi.WORKSPACE_NEXT -> out.workspace(1)
            TapRules.Multi.NONE -> {}
        }
    }

    /** The touch was taken away (a system gesture, a dialog, the pen): let go of everything. */
    fun cancel() {
        endPinch()
        releaseDrag()
        flushReleases()
        fingers.clear()
        outside.clear()
        resetGesture()
        detectorsMuted = false
        cancelDetectors(0L)
    }

    private fun newGesture() {
        resetGesture()
        sub.reset()
        scrollSub.reset()
    }

    private fun resetGesture() {
        moving = false
        absorb = false
        multiJudged = false
        doubleTapPending = false
        pendingSpan = 0f
    }

    private fun endPinch() {
        if (pinching) {
            pinching = false
            out.pinchEnd()
        }
    }

    private fun releaseDrag() {
        if (dragging) {
            dragging = false
            out.button(LEFT, false)
        }
    }

    /**
     * Give the detectors the event, unless a finger outside the area is down: they would count it,
     * so they are told the touch was cancelled once and left alone until every finger is up.
     */
    private fun feedDetectors(event: MotionEvent) {
        if (outside.isEmpty()) {
            tapDetector.onTouchEvent(event)
            scaleDetector.onTouchEvent(event)
        } else if (!detectorsMuted) {
            detectorsMuted = true
            cancelDetectors(event.eventTime)
        }
    }

    private fun cancelDetectors(time: Long) {
        val cancel = MotionEvent.obtain(time, time, MotionEvent.ACTION_CANCEL, 0f, 0f, 0)
        tapDetector.onTouchEvent(cancel)
        scaleDetector.onTouchEvent(cancel)
        cancel.recycle()
    }

    /** A click: press now, release after [CLICK_GAP_MS]. */
    private fun click(button: Int) {
        out.button(button, true)
        lateinit var release: Runnable
        release = Runnable {
            pendingReleases.remove(release)
            out.button(button, false)
        }
        pendingReleases[release] = button
        handler.postDelayed(release, CLICK_GAP_MS)
    }

    /** Release every click that is still waiting for its release, now. */
    private fun flushReleases() {
        for (r in pendingReleases.keys.toList()) {
            handler.removeCallbacks(r)
            r.run()
        }
    }

    private fun inBounds(x: Float, y: Float) = x in left..right && y in top..bottom

    /** The bottom [TouchpadShape.ZONE_FRACTION] of the area, right half: a tap here is a right click. */
    private fun inRightZone(x: Float, y: Float): Boolean =
        right > left && bottom > top &&
            y >= bottom - (bottom - top) * TouchpadShape.ZONE_FRACTION && x >= (left + right) / 2f

    // MotionEvent.TOOL_TYPE_PALM is @hide; it is 5 in AOSP (0 unknown, 1 finger, 2 stylus, 3 mouse,
    // 4 eraser, 5 palm). Samsung panels report it for a flat hand, which must never act as a finger.
    @Suppress("WrongConstant")
    private fun isPalm(event: MotionEvent, index: Int) = event.getToolType(index) == TOOL_TYPE_PALM

    companion object {
        const val LEFT = 0
        const val RIGHT = 1
        const val MIDDLE = 2

        /** Fingers tracked at once; a sixth is ignored. */
        const val MAX_FINGERS = 5

        /** Travel of one finger before it is a movement, in millimetres. */
        const val SLOP_MM = 2f

        /** Travel of a finger in a multi-finger tap before it is no longer a tap, in millimetres. */
        const val MULTI_SLOP_MM = 4f

        /** Gap between the press and the release of a click. */
        const val CLICK_GAP_MS = 30L

        private const val TOOL_TYPE_PALM = 5
    }
}
