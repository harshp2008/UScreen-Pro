/*
 * Palm rejection for the touchpad (a port of libgestures' palm classifier) and the
 * finger/hardware-state types the touchpad engine uses.
 */
package com.uscreen

import kotlin.math.hypot

/**
 * One finger as the touchpad engine sees it. Positions are millimetres from the top-left of the
 * touch area, sizes are millimetres, time is seconds, which is how libchrome-gestures (the code
 * behind the ChromeOS touchpad) states all of its thresholds.
 */
internal class FingerState(
    val id: Int,
    val x: Float,
    val y: Float,
    /** Long axis of the contact ellipse; 0 when the panel does not report it. */
    val touchMajor: Float = 0f,
    val touchMinor: Float = 0f,
    /** 0..1 as Android reports it; 0 when unknown. */
    val pressure: Float = 0f,
    /** The system itself called this contact a palm (TOOL_TYPE_PALM). */
    val toolPalm: Boolean = false,
)

/** All fingers touching at one instant (a `HardwareState` in libgestures). */
internal class HardwareState(val time: Double, val fingers: List<FingerState>) {
    /** The finger with [id], or null when it is not touching. */
    fun get(id: Int): FingerState? = fingers.firstOrNull { it.id == id }
}

/**
 * Throws out contacts that are a hand and not a fingertip. Modelled on libgestures'
 * `PalmClassifyingFilterInterpreter`:
 *  - too big: `Palm Width` 21.2 mm (a fingertip is about 8 to 14 mm)
 *  - side of the hand: a large, long and thin contact
 *  - pressed very hard (`Palm Pressure`)
 *  - the system said so (TOOL_TYPE_PALM, which Samsung panels do not always set)
 *  - landed in the edge zone (the thumb holding the tablet) and has not gone anywhere:
 *    `Tap Exclusion Border Width` 8 mm; it becomes a pointing finger once it has travelled
 *    `Palm Pointing Min Move Distance` 8 mm
 *
 * A palm stays a palm for as long as that contact lives; it never reaches a gesture, so it can
 * neither click, scroll nor move the pointer.
 */
internal class PalmFilter {
    /**
     * What the filter remembers about one contact: where it landed and whether it is a palm, started in the edge zone, or has become a pointing finger.
     */
    private class Rec(val ox: Float, val oy: Float) {
        var palm = false
        var edge = false
        var pointing = false
    }

    private val recs = HashMap<Int, Rec>()

    /** Width of the touch area in mm, for the edge zones. 0 turns the edge rule off. */
    var padWidthMm = 0f

    /** Ids dropped by the last [filter] call, for the diagnostic log. */
    var lastPalms: Set<Int> = emptySet()
        private set

    /** Forgets every contact, e.g. when the touchpad session restarts. */
    fun reset() {
        recs.clear()
        lastPalms = emptySet()
    }

    /**
     * Returns [hw] without the contacts classified as palms; contacts that lifted are forgotten, and a contact keeps its classification for as long as it lives.
     */
    fun filter(hw: HardwareState): HardwareState {
        recs.keys.retainAll(hw.fingers.map { it.id }.toSet())
        val kept = ArrayList<FingerState>(hw.fingers.size)
        val palms = HashSet<Int>()
        for (f in hw.fingers) {
            val r = recs.getOrPut(f.id) {
                Rec(f.x, f.y).also {
                    it.edge = padWidthMm > 0f &&
                        (f.x < EDGE_ZONE_MM || f.x > padWidthMm - EDGE_ZONE_MM)
                }
            }
            if (!r.palm) {
                if (f.toolPalm ||
                    f.touchMajor >= PALM_WIDTH_MM ||
                    f.pressure >= PALM_PRESSURE ||
                    isSideOfHand(f)
                ) r.palm = true
            }
            if (r.edge && !r.pointing && hypot(f.x - r.ox, f.y - r.oy) >= POINTING_MIN_DIST_MM) {
                r.pointing = true
            }
            if (r.palm || (r.edge && !r.pointing)) palms.add(f.id) else kept.add(f)
        }
        lastPalms = palms
        return HardwareState(hw.time, kept)
    }

    /**
     * A large, long and thin contact (at least 15 mm long and twice as long as wide) is the side of a hand.
     */
    private fun isSideOfHand(f: FingerState): Boolean =
        f.touchMinor > 0f &&
            f.touchMajor >= ELONGATED_MIN_MM &&
            f.touchMajor / f.touchMinor >= ELONGATED_RATIO

    /** Thresholds in millimetres, taken from libgestures' palm classifier (pressure is 0..1). */
    companion object {
        const val PALM_WIDTH_MM = 21.2f
        const val PALM_PRESSURE = 0.98f
        const val EDGE_ZONE_MM = 8f
        const val POINTING_MIN_DIST_MM = 8f
        const val ELONGATED_MIN_MM = 15f
        const val ELONGATED_RATIO = 2f
    }
}
