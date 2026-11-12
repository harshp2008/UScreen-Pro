/*
 * Pen tilt maths: turns Android's stylus tilt axes into the signed X/Y tilt angles a Wacom-style tablet reports.
 * Pure functions with no Android types, so the geometry can be unit tested without a device.
 */
package com.uscreen

import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin

/** Stylus tilt conversion used by [TouchCapture] for every pen sample, live and historical. */
object PenTilt {
    /**
     * Decompose Android's stylus tilt into X/Y tilt angles, **in degrees**.
     *
     * Android exposes AXIS_TILT as the angle from the screen normal (0 =
     * perpendicular, π/2 = flat) and AXIS_ORIENTATION as the azimuth of the
     * tilt around that normal (0..2π). A Wacom-style ABS_TILT_X/Y device wants
     * the signed X and Y tilt *angles*, which are the arctangents of the tilt
     * vector's components projected onto the surface — not the components
     * themselves.
     *
     * The previous version returned the raw projections `sin(tilt)·cos(orient)`
     * (a dimensionless value in [-1,1]) and the host multiplied them by
     * 180/π as if they were radians. A pen laid flat at 90° came out as 57°,
     * and everything in between was wrong non-linearly.
     */
    fun decompose(tiltRad: Double, orientationRad: Double): Pair<Double, Double> {
        val sinTilt = sin(tiltRad)
        val cosTilt = cos(tiltRad)
        // Android's AXIS_ORIENTATION is measured clockwise from the top of the
        // screen: 0 points up, +pi/2 points right. So the direction the pen
        // leans, in screen coordinates with y downwards, is
        // (sin(orientation), -cos(orientation)).
        //
        // libinput wants tilt_x positive towards the right edge and tilt_y
        // positive towards the user, which is the bottom edge — hence the
        // minus on the y component. These two were the other way round until
        // 1.2.2, so a pen leaning right reported as leaning down; reported by
        // a Lenovo Tab Pen Plus user in issue #11.
        val tx = atan2(sinTilt * sin(orientationRad), cosTilt)
        val ty = atan2(-sinTilt * cos(orientationRad), cosTilt)
        return Math.toDegrees(tx) to Math.toDegrees(ty)
    }
}
