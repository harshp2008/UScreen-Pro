/*
 * Touchpad tap-to-click, tap-and-drag and two/three-finger taps: a port of the ChromeOS
 * libgestures state machine.
 */
package com.uscreen

import kotlin.math.hypot

/**
 * Tap to click, tap-and-drag, two and three finger taps. A port of libgestures'
 * `TapToClickManager` (tap_to_click_manager.cc), including its state diagram:
 *
 * ```
 *   Idle --finger down--> FirstTapBegan --all released--> TapComplete --timeout--> click
 *                              |                              |  finger down again
 *                              | moved / too slow             v
 *                              v                       SubsequentTapBegan --moves--> Drag
 *                            Idle                          | released               |  released
 *                                                          v                        v
 *                                                    click, TapComplete    button up, Idle
 * ```
 *
 * What a tap is, and the numbers, come from the original: a tap needs every finger that touched
 * to have lifted, within [Config.tapTimeout]; fingers joining the tap must be close to a finger
 * already in it; one finger is a left click, two a right click, three a middle click.
 *
 * Differences from the original, all on purpose:
 *  - the pressure rules are off (Android's pressure is not a pad's pressure);
 *  - [Config.tapTimeout] is 0.25 s (0.2 on a touchpad): people tap a glass screen more slowly;
 *  - [Config.tapMoveDist] is 3 mm (2 mm) for the same reason, a fingertip rolls on glass;
 *  - four or more fingers are never a tap;
 *  - every state that waits has a timer, so a still finger does not leave the button unpressed.
 */
internal class TapToClick(private val cfg: Config = Config()) {
    /**
     * Thresholds of the tap state machine. Times are seconds, distances are millimetres. `tapTimeout` is how long a tap may last, `interTapTimeout` the wait for a second tap, `tapDragTimeout` the wait to continue a drag, `tapMoveDist` how far a finger may move and still tap, `tapMaxFingerAge` the oldest a finger may be and still count, `tapMinSeparation` the closest two fingers may be to count as separate, `closeEnough` how near a new finger must be to one already in the tap.
     */
    class Config(
        val tapTimeout: Double = 0.25,
        val interTapTimeout: Double = 0.15,
        val tapDragTimeout: Double = 0.3,
        val tapMoveDist: Float = 3f,
        val tapMaxFingerAge: Double = 1.2,
        val motionTapPreventTimeout: Double = 0.05,
        val thumbClickPreventionTimeout: Double = 0.15,
        val tapMinSeparation: Float = 10f,
        val closeEnough: Float = 60f,
        val evaluationTimeout: Double = 0.05,
        val tapDragEnable: Boolean = true,
        val dragLockEnable: Boolean = false,
    )

    /** States of the tap machine, as in libgestures' diagram above. */
    enum class State { IDLE, FIRST_TAP_BEGAN, TAP_COMPLETE, SUBSEQUENT_TAP_BEGAN, DRAG, DRAG_RELEASE, DRAG_RETOUCH }

    /** What to do with the buttons; [timeout] is seconds until [update] should be called again, or -1. */
    class Result(val down: Int = 0, val up: Int = 0, val timeout: Double = -1.0)

    var state = State.IDLE
        private set
    private var stateEntered = -1.0
    private var lastMovement = -1.0
    private var prevGs: Set<Int> = emptySet()
    private val deadFingers = HashSet<Int>()  // fingers that may never tap (kept for parity with libgestures)
    private var dragLastMotion = -1.0
    private var dragFingerStationary = false

    val tapInProgress get() = state == State.FIRST_TAP_BEGAN || state == State.SUBSEQUENT_TAP_BEGAN
    val dragging get() = state == State.DRAG

    /** Where the finger that began the current tap first touched, in mm. */
    val tapOrigin: FingerState? get() = record.touched.values.firstOrNull()

    private val record = TapRecord()

    private inner class TapRecord {
        val touched = LinkedHashMap<Int, FingerState>()
        val released = HashSet<Int>()
        val seen = HashSet<Int>()
        var fingersBelowMaxAge = true

        /** Adds a touching finger to the tap unless it is too far from every finger already in it. */
        fun noteTouch(id: Int, fs: FingerState) {
            // A new finger has to be close enough to one already in the tap.
            if (touched.isNotEmpty() && touched.values.none { dist(it, fs) <= cfg.closeEnough }) return
            touched[id] = fs
        }

        /** Marks a finger of the tap as lifted. */
        fun noteRelease(id: Int) {
            if (id in touched) released.add(id)
        }

        /** Forgets a finger completely. */
        fun remove(id: Int) {
            touched.remove(id)
            released.remove(id)
            seen.remove(id)
        }

        /**
         * Applies one frame: drops dead fingers, adds new ones, marks lifted ones, and notes the fingers that are down; clears `fingersBelowMaxAge` when any finger is older than `tapMaxFingerAge` (`origin` gives each finger's landing time in seconds).
         */
        fun update(
            hw: HardwareState,
            added: Set<Int>,
            removed: Set<Int>,
            dead: Set<Int>,
            origin: (Int) -> Double,
        ) {
            dead.forEach { remove(it) }
            for (id in added) hw.get(id)?.let { noteTouch(id, it) }
            removed.forEach { noteRelease(it) }
            for (id in touched.keys) {
                if (hw.get(id) == null) continue
                seen.add(id)
                if (hw.time - origin(id) > cfg.tapMaxFingerAge) fingersBelowMaxAge = false
            }
        }

        /** Empties the record for the next tap. */
        fun clear() {
            touched.clear()
            released.clear()
            seen.clear()
            fingersBelowMaxAge = true
        }

        /** Whether any finger of the tap has moved more than [max] millimetres from where it landed. */
        fun moving(hw: HardwareState, max: Float): Boolean =
            touched.any { (id, start) ->
                val fs = hw.get(id) ?: return@any false
                dist(fs, start) > max
            }

        /**
         * Whether every finger moved no more than [maxSpeed] millimetres since the previous frame; fingers missing from either frame count as still.
         */
        fun motionless(hw: HardwareState, prev: HardwareState?, maxSpeed: Float): Boolean =
            touched.keys.all { id ->
                val fs = hw.get(id)
                val p = prev?.get(id)
                fs == null || p == null || dist(fs, p) <= maxSpeed
            }

        /** True once any finger has touched. */
        fun tapBegan() = touched.isNotEmpty()
        /** True once every finger that touched has lifted. */
        fun tapComplete() = touched.isNotEmpty() && touched.size == released.size

        /** Left, right or middle by the number of fingers; 0 when it is not a tap we answer. */
        fun tapType(): Int = when (seen.size) {
            0, 1 -> LEFT
            2 -> RIGHT
            3 -> MIDDLE
            else -> NONE
        }
    }

    /** Returns to idle and forgets all fingers, as at the start of a gesture session. */
    fun reset() {
        state = State.IDLE
        stateEntered = -1.0
        lastMovement = -1.0
        prevGs = emptySet()
        deadFingers.clear()
        record.clear()
    }

    /** The pointer moved by more than noise; a tap cannot begin for [Config.motionTapPreventTimeout]. */
    fun noteMovement(time: Double) {
        lastMovement = time
    }

    /** True while a button is held by a drag. */
    val buttonHeld get() = state == State.DRAG || state == State.DRAG_RELEASE || state == State.DRAG_RETOUCH

    /**
     * One step. [hw] is null for a timer firing with no new touch data. [gs] are the ids of the
     * fingers that are gesturing (not palms). [prevGestureWasScroll] cancels taps, as a scroll
     * must never end in a click.
     */
    fun update(
        hw: HardwareState?,
        prev: HardwareState?,
        gs: Set<Int>,
        prevGestureWasScroll: Boolean,
        now: Double,
        origin: (Int) -> Double,
    ): Result {
        var down = 0
        var up = 0
        if (state == State.IDLE && hw == null) return Result()

        if (hw != null) deadFingers.retainAll(hw.fingers.map { it.id }.toSet())

        val added = HashSet<Int>()
        val removed = HashSet<Int>()
        val dead = HashSet<Int>()
        if (hw != null && prevGs != gs) {
            for (id in gs) {
                if (hw.time - origin(id) > cfg.thumbClickPreventionTimeout) continue
                if (id !in prevGs) {
                    val fs = hw.get(id) ?: continue
                    if (tooCloseToTap(hw, fs) || (prev != null && tooCloseToTap(prev, fs)) || id in deadFingers) continue
                    added.add(id)
                }
            }
            for (id in prevGs) {
                if (id in gs) continue
                if (hw.get(id) == null) removed.add(id) else dead.add(id)
            }
        }
        prevGs = gs

        val timedOut = now - stateEntered > timeoutFor(state)

        if (prevGestureWasScroll) {
            // A scroll is not a tap. If it began as a drag, let go of the button.
            if (buttonHeld) up = up or LEFT
            record.clear()
            enter(State.IDLE, now)
            return Result(down, up)
        }

        when (state) {
            State.IDLE -> {
                record.clear()
                if (hw != null && hw.time - lastMovement >= cfg.motionTapPreventTimeout) {
                    record.update(hw, added, removed, dead, origin)
                    if (record.tapBegan()) enter(State.FIRST_TAP_BEGAN, now)
                }
            }

            State.FIRST_TAP_BEGAN -> {
                if (timedOut) {
                    enter(State.IDLE, now)
                } else if (hw != null) {
                    record.update(hw, added, removed, dead, origin)
                    if (record.tapComplete()) {
                        if (!record.fingersBelowMaxAge) {
                            enter(State.IDLE, now)
                        } else if (record.tapType() == NONE) {
                            enter(State.IDLE, now)
                        } else if (record.tapType() == LEFT && cfg.tapDragEnable && !rightZoneHit) {
                            enter(State.TAP_COMPLETE, now)
                        } else {
                            down = record.tapType(); up = down
                            enter(State.IDLE, now)
                        }
                    } else if (record.moving(hw, cfg.tapMoveDist)) {
                        enter(State.IDLE, now)
                    }
                }
            }

            State.TAP_COMPLETE -> {
                if (added.isNotEmpty() && hw != null) {
                    record.clear()
                    record.update(hw, added, removed, dead, origin)
                    if (record.tapType() != LEFT) {
                        // More than one finger came down: the first tap is a click of its own.
                        down = LEFT; up = LEFT
                        enter(State.FIRST_TAP_BEGAN, now)
                    } else {
                        dragLastMotion = now
                        dragFingerStationary = false
                        enter(State.SUBSEQUENT_TAP_BEGAN, now)
                    }
                } else if (timedOut) {
                    down = record.tapType(); up = down
                    enter(State.IDLE, now)
                }
            }

            State.SUBSEQUENT_TAP_BEGAN -> {
                if (hw != null) record.update(hw, added, removed, dead, origin)
                if (hw != null && !record.motionless(hw, prev, 0.0001f)) dragLastMotion = now
                if (record.tapType() == LEFT && now - dragLastMotion >= 0.0) dragFingerStationary = true

                val moving = hw != null && record.moving(hw, cfg.tapMoveDist)
                if (timedOut || moving) {
                    if (record.tapType() == LEFT) {
                        if (timedOut) {
                            // Held down past the timeout: this is a drag.
                            down = LEFT
                            enter(State.DRAG, now)
                        } else if (dragFingerStationary) {
                            down = LEFT
                            enter(State.DRAG, now)
                        } else {
                            down = LEFT; up = LEFT
                            enter(State.IDLE, now)
                        }
                    } else if (!record.tapComplete()) {
                        down = LEFT; up = LEFT
                        enter(State.IDLE, now)
                    }
                } else {
                    if (record.tapType() != LEFT) {
                        down = LEFT; up = LEFT
                        enter(State.FIRST_TAP_BEGAN, now)
                    }
                    if (record.tapComplete()) {
                        down = LEFT; up = LEFT
                        enter(State.TAP_COMPLETE, now)
                    }
                }
            }

            State.DRAG -> {
                if (hw != null) record.update(hw, added, removed, dead, origin)
                if (record.tapComplete()) {
                    record.clear()
                    if (cfg.dragLockEnable) {
                        enter(State.DRAG_RELEASE, now)
                    } else {
                        up = LEFT
                        enter(State.IDLE, now)
                    }
                }
                if (state == State.DRAG && record.tapType() != LEFT && now - stateEntered <= cfg.evaluationTimeout) {
                    // It was not a drag but a multi-finger gesture.
                    up = LEFT
                    enter(State.IDLE, now)
                }
            }

            State.DRAG_RELEASE -> {
                if (added.isNotEmpty() && hw != null) {
                    record.update(hw, added, removed, dead, origin)
                    enter(State.DRAG_RETOUCH, now)
                } else if (timedOut) {
                    up = LEFT
                    enter(State.IDLE, now)
                }
            }

            State.DRAG_RETOUCH -> {
                if (hw != null) record.update(hw, added, removed, dead, origin)
                if (record.tapComplete()) {
                    up = LEFT
                    enter(if (record.tapType() == LEFT) State.IDLE else State.TAP_COMPLETE, now)
                } else if (timedOut) {
                    enter(State.DRAG, now)
                } else if (hw != null && record.moving(hw, cfg.tapMoveDist)) {
                    enter(State.DRAG, now)
                }
            }
        }

        val timeout = if (state == State.IDLE) -1.0 else timeoutFor(state)
        return Result(down, up, timeout)
    }

    /** Set by the engine: the finger that began this tap landed in the right-click zone. */
    var rightZoneHit = false

    /** Moves to [s] and records [now] as the entry time, unless already in [s]. */
    private fun enter(s: State, now: Double) {
        if (state != s) {
            state = s
            stateEntered = now
        }
    }

    /**
     * How long the machine may stay in [s] before the timeout acts: the tap timeout in the touching states, the inter-tap timeout after a completed tap, the drag timeout after a drag release.
     */
    private fun timeoutFor(s: State): Double = when (s) {
        State.IDLE, State.FIRST_TAP_BEGAN, State.SUBSEQUENT_TAP_BEGAN, State.DRAG, State.DRAG_RETOUCH -> cfg.tapTimeout
        State.TAP_COMPLETE -> cfg.interTapTimeout
        State.DRAG_RELEASE -> cfg.tapDragTimeout
    }

    /**
     * Whether another finger is closer than `tapMinSeparation`, which would make [fs] part of a hand rather than a separate tap.
     */
    private fun tooCloseToTap(hw: HardwareState, fs: FingerState): Boolean =
        hw.fingers.any { it.id != fs.id && dist(it, fs) < cfg.tapMinSeparation }

    /** Euclidean distance in millimetres. */
    private fun dist(a: FingerState, b: FingerState) = hypot(a.x - b.x, a.y - b.y)

    /** Button masks: none, left, right and middle. */
    companion object {
        const val NONE = 0
        const val LEFT = 1
        const val RIGHT = 2
        const val MIDDLE = 4
    }
}
