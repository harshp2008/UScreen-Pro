import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

export default class UScreenAppLockExtension extends Extension {
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
        this._setupPenRouting();
        console.log('[uscreen-app-lock] Extension enabled');
    }

    disable() {
        this._teardownPenRouting();
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
        console.log('[uscreen-app-lock] Extension disabled');
    }

    // --- Pen routing (ghost tracking) -------------------------------------------------------
    // The host cannot ask a Wayland compositor which app has focus, so this reports it: the focused
    // window's ids go to ~/.local/share/uscreen/focus.json, and Ctrl+Alt+P asks the host to flip the
    // pen between "graphics tablet" and "mouse" for that app (via ~/.local/share/uscreen/toggle_request).
    _setupPenRouting() {
        this._lastFocusJson = '';
        try {
            this._focusId = global.display.connect('notify::focus-window', () => this._writeFocus());
            this._writeFocus();
        } catch (e) {
            console.error(`[uscreen-app-lock] focus reporting unavailable: ${e}`);
        }
        try {
            this._keySettings = this.getSettings();
            Main.wm.addKeybinding(
                'toggle-tablet',
                this._keySettings,
                Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
                Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
                () => this._toggleTablet()
            );
            this._keyBound = true;
        } catch (e) {
            console.error(`[uscreen-app-lock] Ctrl+Alt+P unavailable (use 'uscreen toggle-tablet'): ${e}`);
        }
    }

    _teardownPenRouting() {
        if (this._focusId) {
            global.display.disconnect(this._focusId);
            this._focusId = null;
        }
        if (this._keyBound) {
            try { Main.wm.removeKeybinding('toggle-tablet'); } catch (e) {}
            this._keyBound = false;
        }
        this._keySettings = null;
    }

    _dataFile(name) {
        let dir = GLib.build_filenamev([GLib.get_home_dir(), '.local', 'share', 'uscreen']);
        GLib.mkdir_with_parents(dir, 0o755);
        return Gio.File.new_for_path(GLib.build_filenamev([dir, name]));
    }

    _writeFocus() {
        let win = global.display.focus_window;
        let data = {app_id: '', wm_class: '', gtk_id: ''};
        if (win && win.get_window_type() === Meta.WindowType.NORMAL) {
            let app = Shell.WindowTracker.get_default().get_window_app(win);
            data = {
                app_id: app ? app.get_id() : '',
                wm_class: win.get_wm_class() || '',
                gtk_id: win.get_gtk_application_id() || '',
            };
        } else if (win) {
            return; // a menu or popup: the app underneath keeps focus for routing
        }
        let json = JSON.stringify(data);
        if (json === this._lastFocusJson) return;
        this._lastFocusJson = json;
        try {
            this._dataFile('focus.json').replace_contents(
                new TextEncoder().encode(json), null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (e) {
            console.error(`[uscreen-app-lock] cannot write focus.json: ${e}`);
        }
    }

    _toggleTablet() {
        try {
            this._dataFile('toggle_request').replace_contents(
                new TextEncoder().encode(`${GLib.get_real_time()}`), null, false,
                Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (e) {
            console.error(`[uscreen-app-lock] cannot write toggle_request: ${e}`);
        }
    }

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

    _getMonitorManager() {
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

    _getMonitorIndex() {
        if (!this._config) return -1;

        // 1. Explicit integer monitor_index if configured
        if (typeof this._config.monitor_index === 'number' && this._config.monitor_index >= 0) {
            return this._config.monitor_index;
        }

        // 2. Query monitor index from connector name
        if (this._config.connector) {
            // Modern GNOME 45+ API via MetaMonitorManager
            try {
                let monManager = this._getMonitorManager();
                if (monManager && typeof monManager.get_monitor_for_connector === 'function') {
                    let idx = monManager.get_monitor_for_connector(this._config.connector);
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
                        if (m && m.connector && (m.connector === this._config.connector || m.connector.includes(this._config.connector))) {
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
                        if (conn && (conn === this._config.connector || conn.includes(this._config.connector))) {
                            return i;
                        }
                    }
                }
            } catch (e) {}
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

    _connectWindowSignals() {
        this._windowCreatedId = global.display.connect('window-created', (display, window) => {
            this._attachWindow(window);
            this._enforceWindow(window);
        });

        this._scanExistingWindows();
    }

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
