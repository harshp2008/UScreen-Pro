package com.uscreen

import android.app.Activity
import android.os.Build
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import io.github.davidallison.android.sdk.penremote.AirMotionEvent
import io.github.davidallison.android.sdk.penremote.ButtonEvent
import io.github.davidallison.android.sdk.penremote.SPenRemote
import io.github.davidallison.android.sdk.penremote.SPenUnit
import io.github.davidallison.android.sdk.penremote.SPenUnitManager
import io.github.davidallison.android.sdk.penremote.SPenUnitType

/**
 * Samsung S Pen Remote (Air Actions) for the "Samsung S Pen Remote" stylus profile, through the
 * clean-room Apache-2.0 SDK in `:spen-remote`. The pen's button and its motion in the air reach the
 * computer as `air_button` and `air_motion` stylus events; the computer's profile decides what they do.
 *
 * It only runs on a Samsung device and while that profile is active: air motion costs battery, so its
 * listener exists only while the button is held. On any other device [start] does nothing.
 */
class SamsungSPen(private val send: (event: String, pressed: Boolean, dx: Double, dy: Double) -> Unit) {
    companion object {
        private const val TAG = "UScreenSPen"

        /** True on a Samsung device, the only place the S Pen Remote service exists. */
        fun isSupportedDevice(): Boolean =
            Build.MANUFACTURER.equals("samsung", ignoreCase = true)

        /**
         * Set once the SDK has said this device has no S Pen Remote (a Samsung tablet whose pen has no
         * Bluetooth / Air Actions, such as the Tab S9 FE+). It is final for the life of the process:
         * retrying on every settings update only repeats the reflection and the error log. Observable,
         * so the settings page can say why the air gestures do nothing.
         */
        var remoteUnsupported by androidx.compose.runtime.mutableStateOf(false)
            private set
    }

    private var unitManager: SPenUnitManager? = null
    private var airUnit: SPenUnit? = null
    private var started = false

    /** Bind to the S Pen service and listen for the button. Idempotent; failures are logged, never thrown. */
    fun start(activity: Activity) {
        if (started || remoteUnsupported || !isSupportedDevice()) return
        started = true
        try {
            SPenRemote.connect(activity, object : SPenRemote.ConnectionResultCallback {
                override fun onSuccess(unitManager: SPenUnitManager) {
                    this@SamsungSPen.unitManager = unitManager
                    listenToButton(unitManager)
                    Log.i(TAG, "S Pen Remote connected")
                }

                override fun onFailure(code: SPenRemote.ConnectionResultCallback.Error) {
                    Log.i(TAG, "S Pen Remote unavailable: $code")
                    started = false
                    if (code == SPenRemote.ConnectionResultCallback.Error.UNSUPPORTED_DEVICE) remoteUnsupported = true
                }
            })
        } catch (t: Throwable) {
            // The SDK reflects into Samsung classes; any failure there must leave the app running.
            Log.w(TAG, "S Pen Remote failed to start", t)
            started = false
        }
    }

    private fun listenToButton(manager: SPenUnitManager) {
        try {
            manager.registerSPenEventListener(manager.getUnit(SPenUnitType.TYPE_BUTTON)) { event ->
                when (ButtonEvent(event).action) {
                    ButtonEvent.ButtonAction.ACTION_DOWN -> {
                        send("air_button", true, 0.0, 0.0)
                        listenToAirMotion(manager)
                    }
                    ButtonEvent.ButtonAction.ACTION_UP -> {
                        stopAirMotion(manager)
                        send("air_button", false, 0.0, 0.0)
                    }
                }
            }
        } catch (t: Throwable) {
            Log.w(TAG, "Could not listen to the S Pen button", t)
        }
    }

    private fun listenToAirMotion(manager: SPenUnitManager) {
        try {
            val unit = manager.getUnit(SPenUnitType.TYPE_AIR_MOTION)
            airUnit = unit
            manager.registerSPenEventListener(unit) { event ->
                val m = AirMotionEvent(event)
                send("air_motion", false, m.deltaX.toDouble(), m.deltaY.toDouble())
            }
        } catch (t: Throwable) {
            Log.w(TAG, "Could not listen to S Pen air motion", t)
        }
    }

    private fun stopAirMotion(manager: SPenUnitManager) {
        try {
            airUnit?.let { manager.unregisterSpenEventListener(it) }
        } catch (t: Throwable) {
            Log.w(TAG, "Could not stop S Pen air motion", t)
        }
        airUnit = null
    }

    /** Release the S Pen service. Idempotent. */
    fun stop(activity: Activity) {
        if (!started) return
        started = false
        try {
            unitManager?.let { stopAirMotion(it) }
            SPenRemote.disconnect(activity)
        } catch (t: Throwable) {
            Log.w(TAG, "S Pen Remote did not disconnect cleanly", t)
        }
        unitManager = null
    }
}
