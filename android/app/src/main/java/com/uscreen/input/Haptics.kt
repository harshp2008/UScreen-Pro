/*
 * The tick that confirms a touchpad click, with a strength, and a check for whether the device has a vibration motor at all.
 */
package com.uscreen

import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

// PLATFORM(android): Vibrator and VibrationEffect, which exist from different API levels
/**
 * The tick that confirms a touchpad click. Uses [VibratorManager] on Android 12+ and the older [Vibrator] service
 * before that.
 *
 * Measured on a Galaxy Tab S9 FE+ (SM-X616B): Android reports no vibrator at all there (`vibratorIds = []`, and
 * `hasVibrator()` is false), so nothing can be felt on that tablet whatever the code does. [hasMotor] lets the
 * settings say so instead of staying silent. Where a motor exists the tick is the system's own effect for the
 * chosen strength, else a pulse whose length grows with the strength.
 */
object Haptics {
    /** Pulse length when the hardware has no built-in effect, by strength. */
    private fun pulseMs(strength: String) = when (strength) {
        STRONG -> 90L
        LIGHT -> 30L
        else -> 55L
    }

    /**
     * The system vibrator: the default one of the `VibratorManager` on Android 12 (S) and later, the legacy service before that; null when the device has none.
     */
    private fun vibrator(context: Context): Vibrator? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(VibratorManager::class.java)?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        }

    /** Whether this device has a vibration motor Android knows about. */
    fun hasMotor(context: Context): Boolean = try {
        vibrator(context)?.hasVibrator() == true
    } catch (_: Exception) {
        false
    }

    /** The effect for one click of [strength] on [v]. */
    private fun clickEffect(v: Vibrator, strength: String): VibrationEffect {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val builtIn = when (strength) {
                STRONG -> VibrationEffect.EFFECT_HEAVY_CLICK
                LIGHT -> VibrationEffect.EFFECT_TICK
                else -> VibrationEffect.EFFECT_CLICK
            }
            if (v.areAllEffectsSupported(builtIn) == Vibrator.VIBRATION_EFFECT_SUPPORT_YES) {
                return VibrationEffect.createPredefined(builtIn)
            }
        }
        val amplitude = if (v.hasAmplitudeControl()) {
            when (strength) {
                STRONG -> 255
                LIGHT -> 90
                else -> 170
            }
        } else {
            VibrationEffect.DEFAULT_AMPLITUDE
        }
        return VibrationEffect.createOneShot(pulseMs(strength), amplitude)
    }

    /** One click tick of [strength] (`light`, `medium` or `strong`). Safe to call from any thread. */
    fun click(context: Context, strength: String = MEDIUM) {
        try {
            val v = vibrator(context)?.takeIf { it.hasVibrator() } ?: return
            v.vibrate(clickEffect(v, strength))
        } catch (_: Exception) {
            // Vibration is a nicety; never let it break input.
        }
    }

    const val LIGHT = "light"
    const val MEDIUM = "medium"
    const val STRONG = "strong"
    val STRENGTHS = listOf(LIGHT, MEDIUM, STRONG)
}
