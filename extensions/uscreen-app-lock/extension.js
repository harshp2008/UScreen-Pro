/*
 * UScreen GNOME Shell extension (runs inside gnome-shell, GNOME 45+ module format).
 *
 * Three jobs for the UScreen daemon, which cannot ask a Wayland compositor for any of this:
 *   1. App lock: keep chosen apps on the virtual display (config in app_lock.json).
 *   2. Pen routing: report the focused app (focus.json) and the Ctrl+Alt+P route hotkey
 *      (toggle_request), so the pen can be a graphics tablet in Krita and a mouse in Chrome.
 *   3. Context probe: on request (context_request) report where the pointer is and how much text
 *      is selected / on the clipboard (context.json), for the pen's long-press menu.
 *   4. Pen menu: draw the long-press menu (cut / copy / paste / ...) inside the Shell on request
 *      (menu_request.json), so it is not a window: nothing in the dock, no focus change. The
 *      answer goes to menu_shown (on screen) and menu_result (the choice).
 * All files live in ~/.local/share/uscreen/.
 */
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import St from 'gi://St';

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
        this._setupContextProbe();
        this._setupPenMenu();
        console.log('[uscreen-app-lock] Extension enabled');
    }

    disable() {
        this._teardownPenMenu();
        this._teardownContextProbe();
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

    // --- Context probe (pen long-press menu) --------------------------------------------------
    // The host writes ~/.local/share/uscreen/context_request when the pen was held still and lifted.
    // This answers in ~/.local/share/uscreen/context.json with what the menu needs to decide whether
    // (and what) to show: where the pointer is (a window, the panel/dock/shell UI, the desktop, the
    // overview), the window's frame, and how much text is highlighted / on the clipboard. Only
    // lengths are reported, never the text itself.
    _setupContextProbe() {
        this._ctxBusy = false;
        try {
            let file = this._dataFile('context_request');
            this._ctxMonitor = file.monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._ctxMonitorId = this._ctxMonitor.connect('changed', (mon, f, other, ev) => {
                if (ev === Gio.FileMonitorEvent.CHANGES_DONE_HINT || ev === Gio.FileMonitorEvent.CREATED)
                    this._probeContext();
            });
        } catch (e) {
            console.error(`[uscreen-app-lock] context probe unavailable: ${e}`);
        }
    }

    _teardownContextProbe() {
        if (this._ctxMonitor) {
            try { this._ctxMonitor.disconnect(this._ctxMonitorId); } catch (e) {}
            this._ctxMonitor.cancel();
            this._ctxMonitor = null;
        }
        if (this._ctxTimeout) {
            GLib.source_remove(this._ctxTimeout);
            this._ctxTimeout = null;
        }
    }

    _zoneAt(x, y) {
        // zone: 'overview' | 'shell' (panel, dock, popups) | 'window' | 'desktop'; plus the window.
        let out = {zone: 'desktop', frame: null, csd: false, app_id: '', wm_class: ''};
        try {
            if (Main.overview.visible) { out.zone = 'overview'; return out; }
            let actor = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x, y);
            let inWindows = false, windowActor = null;
            for (let a = actor; a; a = a.get_parent()) {
                if (a === global.window_group) inWindows = true;
                if (!windowActor && a.meta_window) windowActor = a;
            }
            if (windowActor && inWindows) {
                let w = windowActor.meta_window;
                let r = w.get_frame_rect();
                out.zone = 'window';
                out.frame = {x: r.x, y: r.y, w: r.width, h: r.height};
                out.csd = w.is_client_decorated();
                out.wm_class = w.get_wm_class() || '';
                let app = Shell.WindowTracker.get_default().get_window_app(w);
                out.app_id = app ? app.get_id() : '';
            } else if (actor && actor !== global.stage) {
                // Something of the Shell itself unless it is the wallpaper/background layer.
                let isBackground = false;
                for (let a = actor; a; a = a.get_parent()) {
                    if (Main.layoutManager._backgroundGroup && a === Main.layoutManager._backgroundGroup) isBackground = true;
                }
                out.zone = isBackground ? 'desktop' : 'shell';
            }
        } catch (e) {
            console.error(`[uscreen-app-lock] zone lookup failed: ${e}`);
        }
        return out;
    }

    _probeContext() {
        if (this._ctxBusy) return;
        this._ctxBusy = true;
        let [x, y] = global.get_pointer();
        let result = Object.assign({t: GLib.get_real_time(), x, y, selection_len: 0, clipboard_len: 0},
                                   this._zoneAt(x, y));
        let pending = 2, written = false;
        let finish = () => {
            if (written) return;
            written = true;
            this._ctxBusy = false;
            if (this._ctxTimeout) { GLib.source_remove(this._ctxTimeout); this._ctxTimeout = null; }
            try {
                this._dataFile('context.json').replace_contents(
                    new TextEncoder().encode(JSON.stringify(result)), null, false,
                    Gio.FileCreateFlags.REPLACE_DESTINATION, null);
            } catch (e) {
                console.error(`[uscreen-app-lock] cannot write context.json: ${e}`);
            }
        };
        let got = () => { if (--pending === 0) finish(); };
        try {
            let cb = St.Clipboard.get_default();
            cb.get_text(St.ClipboardType.PRIMARY, (c, text) => { result.selection_len = text ? text.length : 0; got(); });
            cb.get_text(St.ClipboardType.CLIPBOARD, (c, text) => { result.clipboard_len = text ? text.length : 0; got(); });
        } catch (e) {
            console.error(`[uscreen-app-lock] clipboard read failed: ${e}`);
            finish();
            return;
        }
        // A clipboard owner that never answers must not leave the host waiting.
        this._ctxTimeout = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 150, () => { this._ctxTimeout = null; finish(); return GLib.SOURCE_REMOVE; });
    }

    // --- Pen menu -----------------------------------------------------------------------------
    // The host writes ~/.local/share/uscreen/menu_request.json ({id, items, style, timeout_ms}); the menu
    // is drawn here as Shell chrome at the pointer, `menu_shown` gets the id as soon as it is up, and
    // `menu_result` gets {id, action} when a button is tapped ("none" if dismissed or timed out).
    _setupPenMenu() {
        this._menuBox = null;
        this._menuId = null;
        this._menuCapture = null;
        this._menuTimeout = null;
        try {
            // The host asks for the menu to close when a tap elsewhere ended (the shell cannot see
            // taps that go to app windows).
            this._closeMonitor = this._dataFile('menu_close').monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._closeMonitorId = this._closeMonitor.connect('changed', (mon, f, other, ev) => {
                if (ev !== Gio.FileMonitorEvent.CHANGES_DONE_HINT && ev !== Gio.FileMonitorEvent.CREATED)
                    return;
                try {
                    let [ok, bytes] = f.load_contents(null);
                    this._finishMenu(Number(new TextDecoder().decode(bytes).trim()), 'none');
                } catch (e) {}
            });
            let file = this._dataFile('menu_request.json');
            this._menuMonitor = file.monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._menuMonitorId = this._menuMonitor.connect('changed', (mon, f, other, ev) => {
                if (ev === Gio.FileMonitorEvent.CHANGES_DONE_HINT || ev === Gio.FileMonitorEvent.CREATED)
                    this._onMenuRequest();
            });
        } catch (e) {
            console.error(`[uscreen-app-lock] pen menu unavailable: ${e}`);
        }
    }

    _teardownPenMenu() {
        this._removeMenu();
        if (this._closeMonitor) {
            try { this._closeMonitor.disconnect(this._closeMonitorId); } catch (e) {}
            this._closeMonitor.cancel();
            this._closeMonitor = null;
        }
        if (this._menuMonitor) {
            try { this._menuMonitor.disconnect(this._menuMonitorId); } catch (e) {}
            this._menuMonitor.cancel();
            this._menuMonitor = null;
        }
    }

    _onMenuRequest() {
        let req;
        try {
            let [ok, bytes] = this._dataFile('menu_request.json').load_contents(null);
            req = JSON.parse(new TextDecoder().decode(bytes));
        } catch (e) {
            return;
        }
        if (!req || !Array.isArray(req.items) || req.items.length === 0 || req.id === this._menuId)
            return;
        try {
            this._removeMenu();
            this._showMenu(req);
        } catch (e) {
            console.error(`[uscreen-app-lock] cannot show the pen menu: ${e}`);
            this._removeMenu();
        }
    }

    _showMenu(req) {
        const LABELS = {'cut': 'Cut', 'copy': 'Copy', 'paste': 'Paste', 'select-all': 'Select all', 'undo': 'Undo'};
        const ICONS = {
            'cut': 'edit-cut-symbolic', 'copy': 'edit-copy-symbolic', 'paste': 'edit-paste-symbolic',
            'select-all': 'edit-select-all-symbolic', 'undo': 'edit-undo-symbolic',
        };
        let box = new St.BoxLayout({style_class: 'uscreen-menu', reactive: true});
        for (let name of req.items) {
            let button = new St.Button({style_class: 'uscreen-menu-button', reactive: true, can_focus: false, track_hover: true});
            if (req.style === 'icons')
                button.child = new St.Icon({icon_name: ICONS[name] || 'dialog-question-symbolic', style_class: 'uscreen-menu-icon'});
            else
                button.label = LABELS[name] || name;
            button.connect('clicked', () => this._finishMenu(req.id, name));
            box.add_child(button);
        }
        Main.layoutManager.addTopChrome(box);
        this._menuBox = box;
        this._menuId = req.id;

        // Centred above the pointer, kept inside the work area; below it when there is no room above.
        let [px, py] = global.get_pointer();
        let [, w] = box.get_preferred_width(-1);
        let [, h] = box.get_preferred_height(w);
        let area = Main.layoutManager.getWorkAreaForMonitor(global.display.get_current_monitor());
        let x = Math.max(area.x + 8, Math.min(px - w / 2, area.x + area.width - w - 8));
        let y = py - 64 - h;
        if (y < area.y + 8) y = py + 28;
        box.set_position(Math.round(x), Math.round(y));

        // A tap anywhere else dismisses it.
        this._menuCapture = global.stage.connect('captured-event', (stage, event) => {
            let type = event.type();
            if (type === Clutter.EventType.BUTTON_PRESS || type === Clutter.EventType.TOUCH_BEGIN) {
                let [ex, ey] = event.get_coords();
                let [bx, by] = box.get_transformed_position();
                let [bw, bh] = box.get_size();
                if (ex < bx || ex > bx + bw || ey < by || ey > by + bh)
                    this._finishMenu(req.id, 'none');
            }
            return Clutter.EVENT_PROPAGATE;
        });
        this._menuTimeout = GLib.timeout_add(GLib.PRIORITY_DEFAULT, req.timeout_ms || 8000, () => {
            this._menuTimeout = null;
            this._finishMenu(req.id, 'none');
            return GLib.SOURCE_REMOVE;
        });
        this._writeFile('menu_shown', `${req.id}`);
    }

    _finishMenu(id, action) {
        if (!this._menuBox || this._menuId !== id) return;
        this._removeMenu();
        this._writeFile('menu_result', JSON.stringify({id, action}) + '\n');
    }

    _removeMenu() {
        if (this._menuTimeout) { GLib.source_remove(this._menuTimeout); this._menuTimeout = null; }
        if (this._menuCapture) { global.stage.disconnect(this._menuCapture); this._menuCapture = null; }
        if (this._menuBox) {
            Main.layoutManager.removeChrome(this._menuBox);
            this._menuBox.destroy();
            this._menuBox = null;
        }
    }

    _writeFile(name, text) {
        try {
            this._dataFile(name).replace_contents(
                new TextEncoder().encode(text), null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (e) {
            console.error(`[uscreen-app-lock] cannot write ${name}: ${e}`);
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
