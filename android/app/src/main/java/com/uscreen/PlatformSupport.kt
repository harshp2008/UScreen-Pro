/*
 * Small platform helpers: the device's Wi-Fi address and whether touch on the lock screen is
 * supported.
 */
package com.uscreen

import android.content.Context
import android.os.Build
import java.net.Inet4Address
import java.net.NetworkInterface
import java.util.Collections

/*
 * Small platform queries that do not belong to any one screen: they are used by the activity and by
 * the troubleshooting UI, and kept apart so MainActivity stays about lifecycle and window policy.
 */

// PLATFORM(android): assumes the Wi-Fi interface is named wlan*, which holds on most but not all devices
/**
 * Retrieve the active IPv4 address assigned to the device's wireless (`wlan`) interface.
 */
fun getLocalWifiIp(@Suppress("UNUSED_PARAMETER") context: Context? = null): String? {
    try {
        val interfaces = Collections.list(NetworkInterface.getNetworkInterfaces())
        for (intf in interfaces) {
            if (intf.name.startsWith("wlan")) {
                for (addr in Collections.list(intf.inetAddresses)) {
                    if (!addr.isLoopbackAddress && addr is Inet4Address) {
                        return addr.hostAddress
                    }
                }
            }
        }
    } catch (_: Exception) {}
    return null
}

// PLATFORM(android): Android 8.1 (O_MR1) or later
/**
 * Whether touches can reach the app while the keyguard is up. The activity can only be drawn over the
 * lock screen through `setShowWhenLocked`, which arrived in API 27; below that the window is never
 * shown while locked, so there is nothing to touch.
 */
fun lockScreenTouchSupported(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1
