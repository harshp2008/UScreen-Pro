/*
 * Holds back the first frames of a new touch so a tap can be dropped before the computer ever sees it (the Classic touchpad without tap-to-click).
 */
package com.uscreen

import kotlin.math.hypot

/**
 * Sits between the touch frames and the computer. A new touch is held until it moves more than [slopMm] or lasts longer
 * than [tapMs]; if every finger lifts before that, the whole touch is dropped, so libinput never sees a tap. Once a
 * touch has been let through its frames pass at once until all fingers lift. [F] is a frame; [points] reads the
 * fingers of one (empty = all lifted), in millimetres.
 */
class TapGate<F>(
    private val points: (F) -> List<NativeTapDetector.Point>,
    private val tapMs: Long = NativeTapDetector.TAP_MS,
    private val slopMm: Float = NativeTapDetector.SLOP_MM,
) {
    private val held = ArrayList<F>()
    private val origin = HashMap<Int, NativeTapDetector.Point>()
    private var sinceMs = 0L
    private var released = false

    /** Offer [frame] seen at [nowMs]; returns the frames to send now, in order (none while held or dropped). */
    fun offer(frame: F, nowMs: Long): List<F> {
        val pts = points(frame)
        if (pts.isEmpty()) {
            val pass = released
            reset()
            return if (pass) listOf(frame) else emptyList()
        }
        if (released) return listOf(frame)
        if (held.isEmpty()) sinceMs = nowMs
        held.add(frame)
        var moved = nowMs - sinceMs > tapMs
        for (p in pts) {
            val first = origin.getOrPut(p.id) { p }
            if (hypot(p.x - first.x, p.y - first.y) > slopMm) moved = true
        }
        if (!moved) return emptyList()
        released = true
        val out = held.toList()
        held.clear()
        return out
    }

    /** Forget the current touch. */
    fun reset() {
        held.clear()
        origin.clear()
        released = false
    }
}
