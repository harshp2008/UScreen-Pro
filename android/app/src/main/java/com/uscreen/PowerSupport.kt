package com.uscreen

import android.animation.ValueAnimator
import android.app.admin.DeviceAdminReceiver
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.provider.Settings
import android.util.Log
import android.view.Window
import android.view.WindowManager
import kotlin.math.abs

/*
 * Power handling of the tablet: fading the screen for the host's idle dimming, reading how fast the
 * battery charges, and locking the device once battery saver gives up on the host.
 */

/**
 * Fades the window's own brightness (`screenBrightness`), which needs no permission and affects
 * only this app's window. Call every method on the UI thread.
 */
class ScreenDimmer(private val window: () -> Window?, private val context: Context) {
    private var animator: ValueAnimator? = null

    /** Fade down to [FadeCurve.DIM_LEVEL] over [durationMs] along [curve]. */
    fun dim(durationMs: Long, curve: String) = fadeTo(FadeCurve.DIM_LEVEL, durationMs, curve)

    /** Fade back up to [FadeCurve.FULL_LEVEL]; starting from the current level avoids a flash. */
    fun undim(durationMs: Long, curve: String) = fadeTo(FadeCurve.FULL_LEVEL, durationMs, curve)

    /** Jump to [level] at once (battery saver's 10%). */
    fun setNow(level: Float) {
        animator?.cancel()
        animator = null
        apply(level)
    }

    /** Give brightness back to the system setting. */
    fun restoreSystem() = setNow(WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE)

    private fun fadeTo(target: Float, durationMs: Long, curve: String) {
        val from = currentLevel()
        animator?.cancel()
        if (durationMs <= 0 || from == target) {
            animator = null
            apply(target)
            return
        }
        animator = ValueAnimator.ofFloat(from, target).apply {
            duration = durationMs
            interpolator = FadeCurve.interpolator(curve)
            addUpdateListener { apply(it.animatedValue as Float) }
            start()
        }
    }

    /** The window's brightness now; with no override set, the system brightness the panel is really at. */
    private fun currentLevel(): Float {
        val set = window()?.attributes?.screenBrightness ?: WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE
        if (set >= 0f) return set
        return try {
            Settings.System.getInt(context.contentResolver, Settings.System.SCREEN_BRIGHTNESS, 128) / 255f
        } catch (_: Exception) {
            0.5f
        }
    }

    private fun apply(level: Float) {
        val w = window() ?: return
        val lp = w.attributes
        lp.screenBrightness = level
        w.attributes = lp
    }
}

/** What the settings sheet says about charging. */
object ChargingStatus {
    /** Below this a tablet with its screen on barely gains charge. */
    const val SLOW_MA = 300

    /**
     * Describe charging from the battery broadcast's `plugged` / `status` extras and the instantaneous
     * current in microamps (`BATTERY_PROPERTY_CURRENT_NOW`, null or 0 when the device does not report it).
     *
     * Vendors disagree on the sign of the current and some report milliamps instead of microamps, so the
     * magnitude is used and anything under 20 000 is taken as already being in milliamps.
     */
    fun describe(plugged: Int, status: Int, currentNow: Long?): String {
        if (plugged == 0) return "On battery"
        if (status == BatteryManager.BATTERY_STATUS_FULL) return "Fully charged"
        val raw = currentNow?.takeIf { it != 0L && it != Long.MIN_VALUE }?.let { abs(it) }
        if (raw != null) {
            val ma = if (raw < 20_000) raw else raw / 1000
            return if (ma < SLOW_MA) "Charging (Slow): ${ma}mA" else "Charging: ${ma}mA"
        }
        // No current reading: a plain USB port (not a wall charger or a wireless pad) is the slow case.
        return if (plugged == BatteryManager.BATTERY_PLUGGED_USB) "Charging (Slow)" else "Charging"
    }

    /** Read the battery now and describe it. */
    fun read(context: Context): String {
        val battery = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
        val plugged = battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) ?: 0
        val status = battery?.getIntExtra(BatteryManager.EXTRA_STATUS, -1) ?: -1
        val bm = context.getSystemService(Context.BATTERY_SERVICE) as? BatteryManager
        val current = try {
            bm?.getLongProperty(BatteryManager.BATTERY_PROPERTY_CURRENT_NOW)
        } catch (_: Exception) {
            null
        }
        return describe(plugged, status, current)
    }
}

/** Device-admin receiver; holding the admin role is what lets the app lock the screen (`force-lock` policy only). */
class UScreenAdminReceiver : DeviceAdminReceiver()

/** Locks the tablet for battery saver. */
object ScreenLock {
    private fun component(context: Context) = ComponentName(context, UScreenAdminReceiver::class.java)

    private fun dpm(context: Context) =
        context.getSystemService(Context.DEVICE_POLICY_SERVICE) as DevicePolicyManager

    /** Whether the user has granted this app the device-admin role. */
    fun canLock(context: Context): Boolean = dpm(context).isAdminActive(component(context))

    /** The system screen that asks the user to grant the role. */
    fun requestIntent(context: Context): Intent =
        Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
            .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, component(context))
            .putExtra(
                DevicePolicyManager.EXTRA_ADD_EXPLANATION,
                "UScreen uses this only to lock the tablet when battery saver gives up on a host that stays offline."
            )

    /** Lock now. Returns false (and does nothing) when the role has not been granted. */
    fun lockNow(context: Context): Boolean {
        if (!canLock(context)) return false
        return try {
            dpm(context).lockNow()
            true
        } catch (e: Exception) {
            Log.w("UScreen", "lockNow failed: ${e.message}")
            false
        }
    }
}
