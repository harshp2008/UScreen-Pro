/*
 * Broadcast receiver behind the desktop's "Identify" button: posts a high-priority notification
 * naming this device so the user can tell which tablet is which.
 */
package com.uscreen

import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build

/**
 * Posts a heads-up "this is your target device" notification when the desktop's Identify button asks
 * for it (`adb shell am broadcast -n com.uscreen/.IdentifyReceiver --es name <device name>`).
 *
 * `cmd notification post` cannot set a priority, so its notifications never pop up; this posts on an
 * IMPORTANCE_HIGH channel instead. The manifest guards the receiver with the DUMP permission, which
 * only the shell and the system hold, so other apps cannot trigger it.
 */
class IdentifyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val name = intent.getStringExtra("name")?.trim().takeUnless { it.isNullOrEmpty() } ?: Build.MODEL
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "Identify device", NotificationManager.IMPORTANCE_HIGH).apply {
                description = "Shown when you press Identify on the computer"
                enableVibration(true)
            }
        )
        val open = PendingIntent.getActivity(
            context, 0, Intent(context, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE
        )
        val text = "This is your target device!"
        val note = Notification.Builder(context, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("UScreen: $name")
            .setContentText(text)
            .setStyle(Notification.BigTextStyle().bigText(text))
            .setCategory(Notification.CATEGORY_ALARM)
            .setContentIntent(open)
            .setAutoCancel(true)
            .setTimeoutAfter(30_000)
            .build()
        val shown = nm.areNotificationsEnabled()
        if (shown) nm.notify(NOTIFICATION_ID, note)
        if (isOrderedBroadcast) {
            resultCode = if (shown) Activity.RESULT_OK else Activity.RESULT_CANCELED
            resultData = if (shown) "posted" else "blocked"
        }
    }

    companion object {
        private const val CHANNEL_ID = "uscreen_identify"
        private const val NOTIFICATION_ID = 2
    }
}
