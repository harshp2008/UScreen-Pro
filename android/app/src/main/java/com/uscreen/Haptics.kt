/*
 * The short vibration that confirms a touchpad click.
 */
package com.uscreen

import android.content.Context
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager

/**
 * The tick that confirms a touchpad click. Uses [VibratorManager] on Android 12+ and the
 * older [Vibrator] service before that; a tablet without a vibration motor simply gets nothing.
 *
 * Measured on a Galaxy Tab S9 FE (SM-X616B): its vibrator reports no amplitude control
 * (`capabilitiesFlags = 0`), so the old 50 ms pulse "at amplitude 80" was one short, fixed-strength
 * burst that is barely felt. The tick is now the system's own heavy click where the vibrator
 * supports it, and otherwise a longer pulse at full strength.
 */
object Haptics {
    /** Length of the fallback pulse, long enough for a tablet motor to spin up and be felt. */
    const val CLICK_MS = 70L

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

    /** The effect for one click on [v]: the heavy click if the hardware has it, else a full-strength pulse. */
    private fun clickEffect(v: Vibrator): VibrationEffect {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
            v.areAllEffectsSupported(VibrationEffect.EFFECT_HEAVY_CLICK) == Vibrator.VIBRATION_EFFECT_SUPPORT_YES
        ) {
            return VibrationEffect.createPredefined(VibrationEffect.EFFECT_HEAVY_CLICK)
        }
        val amplitude = if (v.hasAmplitudeControl()) 255 else VibrationEffect.DEFAULT_AMPLITUDE
        return VibrationEffect.createOneShot(CLICK_MS, amplitude)
    }

    /** One click tick. Safe to call from any thread. */
    fun click(context: Context) {
        try {
            val v = vibrator(context)?.takeIf { it.hasVibrator() } ?: return
            v.vibrate(clickEffect(v))
        } catch (_: Exception) {
            // Vibration is a nicety; never let it break input.
        }
    }
}
