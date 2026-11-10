package com.uscreen

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * Lets the computer's installer ask this app to give up its Device Admin role before a reinstall:
 * `adb shell am broadcast -n com.uscreen/.RemoveAdminReceiver`. An active admin cannot be uninstalled,
 * and only the app itself can drop the role. Result code `1` means the role is gone, `0` that it is still held.
 */
class RemoveAdminReceiver : BroadcastReceiver() {
    /** Drops the role, and for an ordered broadcast reports whether it is gone. */
    override fun onReceive(context: Context, intent: Intent) {
        val gone = !ScreenLock.canLock(context) || ScreenLock.removeAdmin(context)
        if (isOrderedBroadcast) resultCode = if (gone) 1 else 0
    }
}
