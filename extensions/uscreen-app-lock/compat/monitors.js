/*
 * compat/monitors.js: finding a monitor by connector name across the GNOME Shell APIs that exist in different versions.
 * Owns every Shell-version difference in monitor lookup: the code tries each API in turn (feature detection), newest first,
 * and each try is isolated so a missing API on one Shell version cannot break the others. Nothing else in the extension
 * touches these APIs.
 */

import Meta from 'gi://Meta';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/**
 * Look up the monitor index of [connector] (for example 'DVI-I-1'; a substring also matches).
 * Order: MetaMonitorManager (GNOME 45+), then Main.layoutManager.monitors, then the display API of GNOME 42-44.
 * Returns -1 when none of them knows the connector.
 */
export function monitorIndexForConnector(connector) {
    // Modern GNOME 45+ API via MetaMonitorManager
    try {
        let monManager = getMonitorManager();
        if (monManager && typeof monManager.get_monitor_for_connector === 'function') {
            let idx = monManager.get_monitor_for_connector(connector);
            if (idx >= 0) {
                return idx;
            }
        }
    } catch (e) {
        console.warn(`[uscreen-app-lock] Error querying get_monitor_for_connector: ${e}`);
    }

    // GNOME LayoutManager monitor array lookup
    try {
        if (Main && Main.layoutManager && Main.layoutManager.monitors) {
            for (let i = 0; i < Main.layoutManager.monitors.length; i++) {
                let m = Main.layoutManager.monitors[i];
                if (m && m.connector && (m.connector === connector || m.connector.includes(connector))) {
                    return i;
                }
            }
        }
    } catch (e) {}

    // Legacy backward compatibility fallback (GNOME 42-44) if get_monitor_connector exists
    try {
        if (typeof global.display.get_monitor_connector === 'function') {
            let n = global.display.get_n_monitors();
            for (let i = 0; i < n; i++) {
                let conn = global.display.get_monitor_connector(i);
                if (conn && (conn === connector || conn.includes(connector))) {
                    return i;
                }
            }
        }
    } catch (e) {}
    return -1;
}

/** The monitor manager of this Shell, from whichever API it offers, or null. */
/**
 * The monitor manager of this GNOME Shell version, from whichever API it offers, or null. Version differences in finding it are handled here.
 */
function getMonitorManager() {
    try {
        if (global.backend && typeof global.backend.get_monitor_manager === 'function') {
            return global.backend.get_monitor_manager();
        }
    } catch (e) {}

    try {
        if (Meta.get_backend && typeof Meta.get_backend === 'function') {
            let backend = Meta.get_backend();
            if (backend && typeof backend.get_monitor_manager === 'function') {
                return backend.get_monitor_manager();
            }
        }
    } catch (e) {}

    return null;
}
