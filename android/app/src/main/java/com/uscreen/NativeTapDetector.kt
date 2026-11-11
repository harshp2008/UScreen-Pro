/*
 * Spots a tap on the touchpad in native-gesture mode, where the computer's libinput makes the click and the tablet never hears of it, so the tablet can still tick.
 */
package com.uscreen

import kotlin.math.hypot
import kotlin.math.max

/**
 * Watches the fingers sent to the computer's native touchpad and says when a tap has just ended: one to three
 * fingers touched briefly without moving, then all lifted. (The computer reads one finger as a left click, two as
 * a right click and three as a middle click.) Positions are in millimetres.
 */
class NativeTapDetector(
    private val tapMs: Long = TAP_MS,
    private val slopMm: Float = SLOP_MM,
) {
    /** One finger on the pad. */
    data class Point(val id: Int, val x: Float, val y: Float)

    private var startMs = 0L
    private var maxFingers = 0
    private var moved = false
    private val firstSeen = HashMap<Int, Point>()

    /**
     * Feed the fingers now on the pad (empty when all are up) at [nowMs]. Returns true on the call that lifts the
     * last finger of a tap.
     */
    fun update(points: List<Point>, nowMs: Long): Boolean {
        if (points.isEmpty()) {
            val tap = firstSeen.isNotEmpty() && !moved && maxFingers in 1..3 && nowMs - startMs <= tapMs
            reset()
            return tap
        }
        if (firstSeen.isEmpty()) startMs = nowMs
        maxFingers = max(maxFingers, points.size)
        for (p in points) {
            val first = firstSeen.getOrPut(p.id) { p }
            if (hypot(p.x - first.x, p.y - first.y) > slopMm) moved = true
        }
        return false
    }

    /** Forget the current touch. */
    fun reset() {
        firstSeen.clear()
        maxFingers = 0
        moved = false
    }

    companion object {
        /** Longest touch that still counts as a tap. */
        const val TAP_MS = 220L

        /** Furthest a finger may drift and still be a tap. */
        const val SLOP_MM = 3f
    }
}
