/*
 * Touchpad gesture engine: finger frames in, mouse actions (move, scroll, pinch, swipes, clicks)
 * out. Pure Kotlin, no Android dependencies, so it is unit-tested on the JVM.
 */
package com.uscreen.input

import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/**
 * Finger frames in, mouse actions out. This is libgestures' (ChromeOS touchpad) pipeline in
 * miniature, with the same thresholds and the same decisions:
 *
 *  - [PalmFilter]            = `PalmClassifyingFilterInterpreter`
 *  - [TapToClick]            = `TapToClickManager`
 *  - this class              = `ImmediateInterpreter` (move, scroll, pinch, three/four finger swipe)
 *
 * It knows nothing about Android. Positions are millimetres, time is seconds, so what a threshold
 * means does not depend on the screen. The caller scales the millimetres that come out.
 *
 * Gesture rules (libgestures names in brackets):
 *  - one finger moves the pointer once it has gone more than [MOVE_REPORT_MM] ("Move Report
 *    Distance"); until a tap has been ruled out, motion is held back and not lost
 *  - after a finger leaves, nothing moves for [CHANGE_TIMEOUT] ("Change Timeout") so that lifting
 *    one of two scrolling fingers cannot make the pointer jump
 *  - two fingers moving the same way scroll; moving apart or together is a pinch (the dot product
 *    of their displacement vectors is negative), and a pinch, once certain, is locked until the
 *    fingers lift ("Pinch Evaluation Timeout" 0.1 s)
 *  - three or four fingers all moving the same way is a swipe: every finger must move in the same
 *    direction and the slowest at least 20% as far as the fastest ("Swipe Distance Ratio")
 *  - taps are [TapToClick]
 */
internal class TouchpadEngine(
    private val out: Output,
    /** Called with a delay in milliseconds when [onTimer] must be called; replaces an earlier request. */
    private val schedule: (Long) -> Unit = {},
) {
    /** Where the engine sends its results. All distances are millimetres on the tablet's touch area. */
    interface Output {
        /** Move the pointer by this much (right and down are positive). */
        fun move(dxMm: Float, dyMm: Float)

        /** 0 = left, 1 = right, 2 = middle. */
        fun button(button: Int, down: Boolean)
        /** Scroll by this much; the sign already includes the natural-scroll setting. */
        fun scroll(dxMm: Float, dyMm: Float)

        /** Change of the distance between two fingers; positive is fingers moving apart. */
        fun pinch(deltaMm: Float)
        /** The two-finger pinch ended; release the zoom modifier. */
        fun pinchEnd()
        /** A [fingers]-finger swipe in [direction], reported once per touch. */
        fun swipe(fingers: Int, direction: Swipe)
    }

    /** Direction of a multi-finger swipe, as the fingers moved on the pad. */
    enum class Swipe { LEFT, RIGHT, UP, DOWN }

    /**
     * The gesture the current set of fingers has been classified as; `NONE` while it is undecided or no finger is down.
     */
    private enum class Type { NONE, MOVE, SCROLL, PINCH, SWIPE }

    /** A position in millimetres. */
    private class Pt(val x: Float, val y: Float)

    /** Content follows the fingers, as on a phone. */
    var naturalScroll = true

    /** Size of the touch area in mm; needed by the right-click zone and the palm edge rule. */
    var padWidthMm = 0f
        set(v) {
            field = v
            palm.padWidthMm = v
        }
    var padHeightMm = 0f

    /** Taps in the bottom-right zone are right clicks. */
    var rightClickZone = false

    /** False in the Classic style without tap-to-click: taps are recognised (so they are not motion) but click nothing. */
    var tapClicks = true

    /** Diagnostic hook; one line per decision. */
    var log: ((String) -> Unit)? = null

    private val palm = PalmFilter()
    private val tap = TapToClick()

    /** Newest first: history[0] is the frame being processed. */
    private val history = ArrayList<HardwareState>()
    private val origin = HashMap<Int, Triple<Double, Float, Float>>()
    private val start = HashMap<Int, Pt>()
    private val swipeStart = HashMap<Int, Pt>()
    private val moveRef = HashMap<Int, Pt>()
    private val scrollRef = HashMap<Int, Pt>()

    private var gs: Set<Int> = emptySet()
    private var gesture = Type.NONE
    private var prevGesture = Type.NONE
    private var gsChanged = -1.0
    private var leaveTime = -1.0

    private var pinchGuessStart = -1.0
    private var pinchGuess = false
    private var pinchLocked = false
    private var pinchPrevDist = -1f

    private var swipeDone = false

    private var lastHw: HardwareState? = null

    /** Feed one snapshot of every finger on the pad. */
    fun onFrame(raw: HardwareState) {
        val now = raw.time
        for (f in raw.fingers) origin.getOrPut(f.id) { Triple(now, f.x, f.y) }
        origin.keys.retainAll(raw.fingers.map { it.id }.toSet())
        val hw = palm.filter(raw)
        process(hw, now)
        if (palm.lastPalms.isNotEmpty()) log?.invoke("palm ids=${palm.lastPalms}")
    }

    /** The timer asked for through `schedule` fired. */
    fun onTimer(now: Double) {
        applyTap(tap.update(null, lastHw, gs, prevGesture == Type.SCROLL, now, ::originTime), now)
    }

    /** Let go of everything: the system took the touch away, or the mode changed. */
    fun cancel() {
        if (tap.buttonHeld) out.button(0, false)
        if (pinchLocked && pinchGuess) out.pinchEnd()
        tap.reset()
        palm.reset()
        history.clear()
        origin.clear()
        start.clear(); swipeStart.clear(); moveRef.clear(); scrollRef.clear()
        gs = emptySet()
        gesture = Type.NONE
        prevGesture = Type.NONE
        resetPinch()
        swipeDone = false
        lastHw = null
    }

    // ---------------------------------------------------------------------------------------

    /** The landing time in seconds of finger [id], or 0 if unknown. */
    private fun originTime(id: Int) = origin[id]?.first ?: 0.0

    /**
     * Handles one frame: updates the finger history (the last 8 frames, newest first), restarts the references when the set of fingers changes, runs the tap machine first (a button change wins over motion in the same frame), then classifies and emits the gesture.
     */
    private fun process(hw: HardwareState, now: Double) {
        history.add(0, hw)
        while (history.size > 8) history.removeAt(history.size - 1)

        val ids = hw.fingers.map { it.id }.toSet()
        if (ids != gs) {
            if (gs.any { it !in ids }) leaveTime = now
            gsChanged = now
            start.clear(); swipeStart.clear(); moveRef.clear(); scrollRef.clear()
            for (f in hw.fingers) {
                val p = Pt(f.x, f.y)
                start[f.id] = p; swipeStart[f.id] = p; moveRef[f.id] = p; scrollRef[f.id] = p
            }
            if (pinchLocked && pinchGuess) out.pinchEnd()
            resetPinch()
            if (gesture != Type.NONE) log?.invoke("fingers ${gs.size}->${ids.size}: gesture $gesture ended")
            gesture = Type.NONE
            if (ids.isEmpty()) swipeDone = false
        }
        gs = ids

        // Tap first, as libgestures does: a button change wins over motion in the same frame.
        tap.rightZoneHit = rightClickZone && inRightZone(tap.tapOrigin)
        val res = tap.update(hw, history.getOrNull(1), ids, prevGesture == Type.SCROLL, now, ::originTime)
        applyTap(res, now)

        updateGesture(hw, now)
        when (gesture) {
            Type.MOVE -> emitMove(hw, now)
            Type.SCROLL -> emitScroll(hw)
            Type.PINCH -> emitPinch(hw)
            Type.SWIPE -> emitSwipe(hw)
            Type.NONE -> {}
        }
        if (gesture != Type.MOVE && !tap.tapInProgress) {
            // Motion that is not reported is dropped, not saved up for later: otherwise the
            // pointer would jump when the hold-off after a lifted finger ends. (While a tap is
            // still possible the motion is kept: it is released if the tap is ruled out.)
            for (f in hw.fingers) moveRef[f.id] = Pt(f.x, f.y)
        }
        prevGesture = gesture
        lastHw = hw
    }

    /**
     * Turns a tap result into button events; a left tap whose first finger landed in the right-click zone becomes a right tap. Schedules the next evaluation when the tap machine asked for a timeout.
     */
    private fun applyTap(res: TapToClick.Result, now: Double) {
        var down = if (tapClicks) res.down else 0
        var up = if (tapClicks) res.up else 0
        if (down == TapToClick.LEFT && up == TapToClick.LEFT && tap.rightZoneHit) {
            down = TapToClick.RIGHT; up = TapToClick.RIGHT
        }
        if (down != 0 || up != 0) log?.invoke("tap state=${tap.state} down=$down up=$up")
        if (down != 0 && down == up) {
            click(down)
        } else {
            for (bit in intArrayOf(TapToClick.LEFT, TapToClick.RIGHT, TapToClick.MIDDLE)) {
                if (down and bit != 0) out.button(buttonIndex(bit), true)
                if (up and bit != 0) out.button(buttonIndex(bit), false)
            }
        }
        if (res.timeout > 0) schedule((res.timeout * 1000).toLong() + 5L)
    }

    /** Presses and releases the button for [bit] at once. */
    private fun click(bit: Int) {
        out.button(buttonIndex(bit), true)
        out.button(buttonIndex(bit), false)
    }

    /** Output button index for a tap button bit: 0 left, 1 right, 2 middle. */
    private fun buttonIndex(bit: Int) = when (bit) {
        TapToClick.RIGHT -> 1
        TapToClick.MIDDLE -> 2
        else -> 0
    }

    /**
     * Whether [f] is in the right-click corner: the right third of the pad, within the bottom `ZONE_FRACTION` of its height. False when the pad size is unknown.
     */
    private fun inRightZone(f: FingerState?): Boolean {
        if (f == null || padWidthMm <= 0f || padHeightMm <= 0f) return false
        return f.x >= padWidthMm * TouchpadShape.CLICKPAD_ZONE_LEFT && f.y >= padHeightMm * (1f - TouchpadShape.ZONE_FRACTION)
    }

    // ---------------------------------------------------------------------------------------
    // Gesture type (ImmediateInterpreter::UpdateCurrentGestureType)

    /**
     * Classifies the gesture (libgestures' UpdateCurrentGestureType) from the number of fingers, their movement and recent finger changes. A drag keeps moving; a pinch and a scroll stay what they are while two fingers remain; a swipe stays a swipe for three or four.
     */
    private fun updateGesture(hw: HardwareState, now: Double) {
        val n = hw.fingers.size
        val before = gesture

        if (tap.dragging || tap.buttonHeld) {
            gesture = if (n >= 1) Type.MOVE else Type.NONE
        } else if (n == 0) {
            gesture = Type.NONE
        } else if (gesture == Type.PINCH && n == 2) {
            // locked
        } else if (gesture == Type.SCROLL && n == 2) {
            // stays a scroll until the fingers change
        } else if (gesture == Type.SWIPE && (n == 3 || n == 4)) {
            gesture = classifyMulti(hw)
            if (gesture == Type.NONE) gesture = Type.SWIPE // keep, fire at most once
        } else if (now < leaveTime + CHANGE_TIMEOUT) {
            gesture = Type.NONE
        } else when (n) {
            1 -> gesture = Type.MOVE
            2 -> {
                val f1 = hw.fingers[0]
                val f2 = hw.fingers[1]
                val settled = gesture == Type.MOVE && now - gsChanged > EVALUATION_TIMEOUT
                if (!settled) {
                    gesture = twoFingerType(hw, f1, f2, now)
                    if (gesture == Type.NONE || gesture == Type.MOVE) {
                        if (updatePinch(hw, f1, f2, now)) {
                            gesture = Type.PINCH
                            pinchPrevDist = dist(start[f1.id], start[f2.id])
                        } else if (earlyZoomPotential(hw, f1, f2, now)) {
                            gesture = Type.NONE
                        }
                    }
                }
            }
            3, 4 -> gesture = classifyMulti(hw)
            else -> gesture = Type.NONE
        }
        if (gesture != before) {
            log?.invoke("gesture $before -> $gesture (fingers=$n)")
            if (gesture == Type.SCROLL) for (f in hw.fingers) scrollRef[f.id] = start[f.id] ?: Pt(f.x, f.y)
        }
    }

    /** ImmediateInterpreter::GetTwoFingerGestureType, without the trend flags and the thumb zone. */
    private fun twoFingerType(hw: HardwareState, f1: FingerState, f2: FingerState, now: Double): Type {
        val s1 = start[f1.id] ?: return Type.NONE
        val s2 = start[f2.id] ?: return Type.NONE
        if (hypot(f1.x - f2.x, f1.y - f2.y) > CLOSE_ENOUGH_MM) return Type.MOVE
        val dx1 = f1.x - s1.x; val dy1 = f1.y - s1.y
        val dx2 = f2.x - s2.x; val dy2 = f2.y - s2.y
        var largeDx = maxMag(dx1, dx2); var smallDx = minMag(dx1, dx2)
        var largeDy = maxMag(dy1, dy2); var smallDy = minMag(dy1, dy2)

        val horizontal = abs(largeDx) > abs(largeDy)
        val large = if (horizontal) largeDx else largeDy
        var small = if (horizontal) smallDx else smallDy
        if (abs(small) < TWO_FINGER_SCROLL_MM) small = 0f
        if (large * small <= 0f) {
            return if (abs(large) < TWO_FINGER_MOVE_MM) Type.NONE else Type.MOVE
        }
        if (abs(large) < TWO_FINGER_SCROLL_MM) return Type.NONE
        if (!scrollAngleOk(f1.id, f2.id)) return Type.NONE
        return Type.SCROLL
    }

    /** ImmediateInterpreter::ScrollAngle: fingers heading in opposite directions are not scrolling. */
    private fun scrollAngleOk(id1: Int, id2: Int): Boolean {
        val old = history[min(history.size - 1, 3)]
        val cur = history[0]
        val a1 = cur.get(id1); val a2 = cur.get(id2); val b1 = old.get(id1); val b2 = old.get(id2)
        if (a1 == null || a2 == null || b1 == null || b2 == null) return true
        return fingersAngle(a1, a2, b1, b2) >= SCROLL_MIN_ANGLE
    }

    /** Cosine of the angle between the two fingers' displacements (0 when they hardly moved). */
    private fun fingersAngle(p1: FingerState, p2: FingerState, c1: FingerState, c2: FingerState): Float {
        val v1x = c1.x - p1.x; val v1y = c1.y - p1.y
        val v2x = c2.x - p2.x; val v2y = c2.y - p2.y
        val dot = v1x * v2x + v1y * v2y
        val size = sqrt((v1x * v1x + v1y * v1y) * (v2x * v2x + v2y * v2y))
        if (size < MIN_DIRECTION_MM * MIN_DIRECTION_MM) return 0f
        return dot / size
    }

    /** Forgets the pinch decision and its reference distance. */
    private fun resetPinch() {
        pinchGuessStart = -1.0
        pinchGuess = false
        pinchLocked = false
        pinchPrevDist = -1f
    }

    /** ImmediateInterpreter::UpdatePinchState: true once the two fingers are known to be pinching. */
    private fun updatePinch(hw: HardwareState, a: FingerState, b: FingerState, now: Double): Boolean {
        if (pinchLocked) return false
        val sa = start[a.id] ?: return false
        val sb = start[b.id] ?: return false
        val d1x = a.x - sa.x; val d1y = a.y - sa.y
        val d2x = b.x - sb.x; val d2y = b.y - sb.y
        val dot = d1x * d2x + d1y * d2y
        val d1sq = d1x * d1x + d1y * d1y
        val d2sq = d2x * d2x + d2y * d2y

        val belowNoise = d1sq + d2sq < PINCH_NOISE_SQ
        val guessMinSq = PINCH_GUESS_MIN * PINCH_GUESS_MIN
        val guessNo = ((d1sq > guessMinSq) xor (d2sq > guessMinSq)) || dot > 0
        val guessYes = (d1sq > guessMinSq || d2sq > guessMinSq) && dot < 0
        val lo = min(d1sq, d2sq); val hi = max(d1sq, d2sq)
        val badRatio = lo <= hi * PINCH_CONSISTENT_RATIO * PINCH_CONSISTENT_RATIO
        var certain = !badRatio && guessYes && !guessNo && zoomFingersConsistent(a.id, b.id)

        if (pinchGuessStart < 0 && !belowNoise) {
            if (guessNo && !guessYes) { pinchGuess = false; pinchGuessStart = now }
            if (guessYes && !guessNo) { pinchGuess = true; pinchGuessStart = now }
        }
        if (pinchGuessStart >= 0) {
            if (pinchGuess != guessYes || pinchGuess == guessNo || belowNoise) {
                pinchGuessStart = -1.0
                return false
            }
            val certSq = PINCH_CERTAIN_MIN * PINCH_CERTAIN_MIN
            certain = certain || (d1sq > certSq && d2sq > certSq && dot < 0)
            val noPinchCertain = (d1sq > certSq || d2sq > certSq) && dot > 0
            pinchGuess = (pinchGuess || certain) && !noPinchCertain
            if (now - pinchGuessStart > PINCH_EVALUATION_TIMEOUT || certain || noPinchCertain) {
                pinchLocked = true
                log?.invoke("pinch decision: pinch=$pinchGuess d1=${"%.1f".format(sqrt(d1sq))} d2=${"%.1f".format(sqrt(d2sq))}")
                return pinchGuess
            }
        }
        return false
    }

    /**
     * Whether the two fingers have moved apart or together consistently over the last few frames, based on the angle between their movements.
     */
    private fun zoomFingersConsistent(id1: Int, id2: Int): Boolean {
        val curr = history[min(history.size - 1, PINCH_ZOOM_MIN_EVENTS)]
        val c1 = curr.get(id1) ?: return false
        val c2 = curr.get(id2) ?: return false
        for (i in 0 until min(history.size, PINCH_ZOOM_MIN_EVENTS)) {
            val p1 = history[i].get(id1) ?: return false
            val p2 = history[i].get(id2) ?: return false
            if (fingersAngle(p1, p2, c1, c2) >= 0f) return false
        }
        val l1 = history[0].get(id1) ?: return false
        val l2 = history[0].get(id2) ?: return false
        return fingersAngle(l1, l2, c1, c2) <= PINCH_ZOOM_MAX_ANGLE
    }

    /** ImmediateInterpreter::EarlyZoomPotential: do not call it a move yet, it may be a pinch. */
    private fun earlyZoomPotential(hw: HardwareState, a: FingerState, b: FingerState, now: Double): Boolean {
        val t1 = originTime(a.id); val t2 = originTime(b.id)
        val early = now - min(t1, t2) < PINCH_EVALUATION_TIMEOUT
        if (!early) return false
        var motionless = false
        for (i in 1 until min(history.size, PINCH_ZOOM_MIN_EVENTS)) {
            val c1 = history[i - 1].get(a.id); val c2 = history[i - 1].get(b.id)
            val p1 = history[i].get(a.id); val p2 = history[i].get(b.id)
            if (c1 == null || c2 == null || p1 == null || p2 == null) { motionless = true; break }
            val m1 = c1.x != p1.x || c1.y != p1.y
            val m2 = c2.x != p2.x || c2.y != p2.y
            if (!m1 && !m2) { motionless = true; break }
        }
        if (motionless) return true
        val o1 = origin[a.id]; val o2 = origin[b.id]
        if (o1 != null && o2 != null) {
            val dot = (a.x - o1.second) * (b.x - o2.second) + (a.y - o1.third) * (b.y - o2.third)
            if (pinchGuessStart > 0 || dot < 0) return true
        }
        return false
    }

    /** ImmediateInterpreter::GetMultiFingerGestureType for three and four fingers. */
    private fun classifyMulti(hw: HardwareState): Type {
        val n = hw.fingers.size
        if (n != 3 && n != 4) return Type.NONE
        if (swipeDone) return Type.NONE
        val byX = hw.fingers.sortedBy { it.x }
        val byY = hw.fingers.sortedBy { it.y }
        val horizontal = (byX.last().x - byX.first().x) >= (byY.last().y - byY.first().y)
        val sorted = if (horizontal) byX else byY
        val dx = FloatArray(n); val dy = FloatArray(n)
        var dxSum = 0f; var dySum = 0f
        for (i in 0 until n) {
            val s = swipeStart[sorted[i].id] ?: return Type.NONE
            dx[i] = sorted[i].x - s.x
            dy[i] = sorted[i].y - s.y
            dxSum += dx[i]; dySum += dy[i]
        }
        val vertical = abs(dxSum) <= abs(dySum)
        val deltas = if (vertical) dy else dx
        for (i in 1 until n) {
            if (deltas[i] * deltas[0] <= 0f) {
                // The fingers disagree: start measuring again from here.
                for (f in sorted) swipeStart[f.id] = Pt(f.x, f.y)
                return Type.NONE
            }
        }
        var maxD = abs(deltas[0]); var minD = abs(deltas[0])
        for (i in 1 until n) { maxD = max(maxD, abs(deltas[i])); minD = min(minD, abs(deltas[i])) }
        if (maxD >= SWIPE_DISTANCE_MM && minD >= SWIPE_RATIO * maxD) return Type.SWIPE
        return Type.NONE
    }

    // ---------------------------------------------------------------------------------------
    // Output

    /**
     * Reports the largest per-finger movement since the last report as pointer movement; does nothing while a tap is still possible, and keeps accumulating until the movement reaches `MOVE_REPORT_MM`.
     */
    private fun emitMove(hw: HardwareState, now: Double) {
        if (tap.tapInProgress) return // a tap has not been ruled out yet; the motion is kept, not lost
        var bestDx = 0f; var bestDy = 0f; var best = 0f
        for (f in hw.fingers) {
            val r = moveRef[f.id] ?: continue
            val dx = f.x - r.x; val dy = f.y - r.y
            val d = hypot(dx, dy)
            if (d > best) { best = d; bestDx = dx; bestDy = dy }
        }
        if (best < MOVE_REPORT_MM) return // accumulates until it is worth reporting
        for (f in hw.fingers) moveRef[f.id] = Pt(f.x, f.y)
        tap.noteMovement(now)
        out.move(bestDx, bestDy)
    }

    /**
     * Scrolls by the average finger movement since the last report, signed by the natural-scroll setting.
     */
    private fun emitScroll(hw: HardwareState) {
        var sx = 0f; var sy = 0f
        for (f in hw.fingers) {
            val r = scrollRef[f.id] ?: continue
            sx += f.x - r.x; sy += f.y - r.y
            scrollRef[f.id] = Pt(f.x, f.y)
        }
        if (sx == 0f && sy == 0f) return
        val n = hw.fingers.size
        val sign = if (naturalScroll) 1f else -1f
        out.scroll(sx / n * sign, sy / n * sign)
    }

    /** Reports the change of finger distance since the last report while a pinch is locked. */
    private fun emitPinch(hw: HardwareState) {
        if (!(pinchLocked && pinchGuess) || hw.fingers.size != 2) return
        val d = hypot(hw.fingers[0].x - hw.fingers[1].x, hw.fingers[0].y - hw.fingers[1].y)
        if (pinchPrevDist < 0f) pinchPrevDist = d
        val delta = d - pinchPrevDist
        if (delta == 0f) return
        pinchPrevDist = d
        out.pinch(delta)
    }

    /**
     * Fires one swipe once the fingers' average travel from where the swipe began reaches `SWIPE_FIRE_MM`; the larger axis decides the direction.
     */
    private fun emitSwipe(hw: HardwareState) {
        if (swipeDone) return
        val n = hw.fingers.size
        var dxSum = 0f; var dySum = 0f
        for (f in hw.fingers) {
            val s = swipeStart[f.id] ?: return
            dxSum += f.x - s.x; dySum += f.y - s.y
        }
        val dx = dxSum / n; val dy = dySum / n
        val vertical = abs(dy) > abs(dx)
        val travelled = if (vertical) abs(dy) else abs(dx)
        if (travelled < SWIPE_FIRE_MM) return
        swipeDone = true
        val dir = if (vertical) { if (dy < 0) Swipe.UP else Swipe.DOWN } else { if (dx < 0) Swipe.LEFT else Swipe.RIGHT }
        log?.invoke("swipe fingers=$n dir=$dir")
        out.swipe(n, dir)
    }

    /** Distance in millimetres between two positions; 0 if either is missing. */
    private fun dist(a: Pt?, b: Pt?): Float =
        if (a == null || b == null) 0f else hypot(a.x - b.x, a.y - b.y)

    /** The value with the larger magnitude. */
    private fun maxMag(a: Float, b: Float) = if (abs(a) > abs(b)) a else b
    /** The value with the smaller magnitude. */
    private fun minMag(a: Float, b: Float) = if (abs(a) < abs(b)) a else b

    /** Default thresholds taken from libgestures' immediate interpreter, in millimetres and seconds. */
    companion object {
        // libgestures defaults (immediate_interpreter.cc), millimetres and seconds
        const val CHANGE_TIMEOUT = 0.2
        const val EVALUATION_TIMEOUT = 0.15
        const val MOVE_REPORT_MM = 0.35f
        const val CLOSE_ENOUGH_MM = 60f
        const val TWO_FINGER_SCROLL_MM = 1.5f
        const val TWO_FINGER_MOVE_MM = 7f
        const val SCROLL_MIN_ANGLE = -0.2f
        const val MIN_DIRECTION_MM = 0.003f
        const val PINCH_NOISE_SQ = 2f
        const val PINCH_GUESS_MIN = 2f
        const val PINCH_CERTAIN_MIN = 8f
        const val PINCH_CONSISTENT_RATIO = 0.4f
        const val PINCH_ZOOM_MIN_EVENTS = 3
        const val PINCH_ZOOM_MAX_ANGLE = -0.4f
        const val PINCH_EVALUATION_TIMEOUT = 0.1
        const val SWIPE_DISTANCE_MM = 1.5f
        const val SWIPE_RATIO = 0.2f

        /** Travel of a swipe before it is reported; libgestures reports continuously, we need one action. */
        const val SWIPE_FIRE_MM = 12f
    }
}
