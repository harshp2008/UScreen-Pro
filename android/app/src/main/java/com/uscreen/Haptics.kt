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
 * The light tick that confirms a touchpad click. Uses [VibratorManager] on Android 12+ and the
 * older [Vibrator] service before that; a tablet without a vibration motor simply gets nothing.
 */
object Haptics {
    /** Length of the tick. */
    const val CLICK_MS = 50L

    /** Strength, 1..255: noticeable but light. */
    private const val CLICK_AMPLITUDE = 80

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

    /** One click tick. Safe to call from any thread. */
    fun click(context: Context) {
        try {
            val v = vibrator(context)?.takeIf { it.hasVibrator() } ?: return
            v.vibrate(VibrationEffect.createOneShot(CLICK_MS, CLICK_AMPLITUDE))
        } catch (_: Exception) {
            // Vibration is a nicety; never let it break input.
        }
    }
}
