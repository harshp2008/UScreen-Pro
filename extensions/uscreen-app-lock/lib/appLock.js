/*
 * lib/appLock.js: the app lock: keeps chosen apps off the virtual display.
 * Owns app_lock.json (written by the daemon), the window hooks and the move of an unauthorised window to the primary monitor.
 * It does not own how a monitor is found in a given Shell version (compat/monitors.js).
 */

import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {monitorIndexForConnector} from '../compat/index.js';

/** The app lock: one instance, started by the extension's enable(). */
export class AppLock {
    /** Starts the config watcher and the window hooks. */
    enable() {
        this._windowCreatedId = null;
        this._windowSignals = new Map();
        this._fileMonitor = null;
        this._config = {
            enabled: false,
            connector: '',
            locked_apps: []
        };

        this._setupConfigMonitor();
        this._connectWindowSignals();
    }

    /** Stops the window hooks and the config watcher. */
    disable() {
        if (this._windowCreatedId) {
            global.display.disconnect(this._windowCreatedId);
            this._windowCreatedId = null;
        }

        for (let [win, id] of this._windowSignals.entries()) {
            try {
                win.disconnect(id);
            } catch (e) {}
        }
        this._windowSignals.clear();

        if (this._fileMonitor) {
            this._fileMonitor.cancel();
            this._fileMonitor = null;
        }
    }


    /** Loads `app_lock.json` and reloads it whenever it changes. */
    _setupConfigMonitor() {
        let home = GLib.get_home_dir();
        let path = `${home}/.local/share/uscreen/app_lock.json`;
        let file = Gio.File.new_for_path(path);

        this._loadConfig(file);

        try {
            this._fileMonitor = file.monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._fileMonitor.connect('changed', (mon, f, other, eventType) => {
                if (eventType === Gio.FileMonitorEvent.CHANGES_DONE_HINT ||
                    eventType === Gio.FileMonitorEvent.CREATED) {
                    this._loadConfig(file);
                }
            });
        } catch (e) {
            console.error(`[uscreen-app-lock] Failed to monitor ${path}: ${e}`);
        }
    }

    /**
     * Reads the app-lock config from [file] and re-checks every window; keeps the old config if the file is missing or invalid.
     */
    _loadConfig(file) {
        try {
            if (!file.query_exists(null)) return;
            let [ok, contents] = file.load_contents(null);
            if (!ok) return;
            let decoder = new TextDecoder();
            let text = decoder.decode(contents);
            this._config = JSON.parse(text);
            console.log(`[uscreen-app-lock] Config updated: enabled=${this._config.enabled}, connector=${this._config.connector}, apps=${JSON.stringify(this._config.locked_apps)}`);
            this._scanExistingWindows();
        } catch (e) {
            console.error(`[uscreen-app-lock] Error reading config: ${e}`);
        }
    }

    /**
     * Index of the monitor the lock applies to: an explicit `monitor_index`, else the monitor of the configured connector (tried through three APIs of different GNOME versions), else, with exactly two monitors, the non-primary one. -1 when unknown.
     */
    _getMonitorIndex() {
        if (!this._config) return -1;

        // 1. Explicit integer monitor_index if configured
        if (typeof this._config.monitor_index === 'number' && this._config.monitor_index >= 0) {
            return this._config.monitor_index;
        }

        // 2. Query monitor index from connector name (Shell-version differences live in compat/monitors.js)
        if (this._config.connector) {
            let byConnector = monitorIndexForConnector(this._config.connector);
            if (byConnector >= 0)
                return byConnector;
        }

        // 3. Fallback: If exactly 2 monitors exist, the non-primary is the virtual target
        try {
            let n = global.display.get_n_monitors();
            const primaryMonitor = global.display.get_primary_monitor();
            if (n === 2) {
                return primaryMonitor === 0 ? 1 : 0;
            }
        } catch (e) {}

        return -1;
    }

    /** Watches new windows and checks every existing one. */
    _connectWindowSignals() {
        this._windowCreatedId = global.display.connect('window-created', (display, window) => {
            this._attachWindow(window);
            this._enforceWindow(window);
        });

        this._scanExistingWindows();
    }

    /** Attaches to and checks the windows of every workspace. */
    _scanExistingWindows() {
        let wsCount = global.workspace_manager.get_n_workspaces();
        for (let w = 0; w < wsCount; w++) {
            let ws = global.workspace_manager.get_workspace_by_index(w);
            let windows = ws.list_windows();
            for (let win of windows) {
                this._attachWindow(win);
                this._enforceWindow(win);
            }
        }
    }

    /** Re-checks [window] whenever it changes monitor; forgets it when it is destroyed. */
    _attachWindow(window) {
        if (!window || this._windowSignals.has(window)) return;
        let id = window.connect('notify::monitor', () => {
            this._enforceWindow(window);
        });
        this._windowSignals.set(window, id);
        window.connect('unmanaged', () => {
            this._windowSignals.delete(window);
        });
    }

    /**
     * If the lock is on and [window] is on the locked monitor but its app is not in `locked_apps` (matched case-insensitively against app id, window class and GTK id), moves it to the primary monitor.
     */
    _enforceWindow(window) {
        if (!this._config.enabled || !this._config.locked_apps || this._config.locked_apps.length === 0) {
            return;
        }
        if (window.get_window_type() !== Meta.WindowType.NORMAL) {
            return;
        }

        let targetMon = this._getMonitorIndex();
        if (targetMon < 0) return;

        let winMon = window.get_monitor();
        if (winMon !== targetMon) return;

        let tracker = Shell.WindowTracker.get_default();
        let app = tracker.get_window_app(window);
        let appId = app ? app.get_id() : '';
        let wmClass = window.get_wm_class() || '';
        let gtkId = window.get_gtk_application_id() || '';

        let allowed = this._config.locked_apps.some(rule => {
            let r = rule.trim().toLowerCase();
            if (!r) return false;
            let rClean = r.replace(/\.desktop$/, '');
            return (appId && (appId.toLowerCase() === r || appId.toLowerCase().includes(rClean)))
                || (wmClass && (wmClass.toLowerCase() === rClean || wmClass.toLowerCase().includes(rClean)))
                || (gtkId && (gtkId.toLowerCase() === rClean || gtkId.toLowerCase().includes(rClean)));
        });

        if (!allowed) {
            const primaryMonitor = global.display.get_primary_monitor();
            if (primaryMonitor !== targetMon) {
                console.log(`[uscreen-app-lock] Moving unauthorized window '${wmClass}' (${appId}) from monitor ${targetMon} to primary ${primaryMonitor}`);
                GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
                    try {
                        window.move_to_monitor(primaryMonitor);
                    } catch (e) {
                        console.error(`[uscreen-app-lock] Failed to move window to primary monitor: ${e}`);
                    }
                    return GLib.SOURCE_REMOVE;
                });
            }
        }
    }
}
