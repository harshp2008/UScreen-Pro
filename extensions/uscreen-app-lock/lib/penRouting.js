/*
 * lib/penRouting.js: pen routing: reports the focused app to the daemon and offers the route hotkey.
 * Owns focus.json (written here) and toggle_request (written on Ctrl+Alt+P); the daemon decides what to do with them.
 */

import Shell from 'gi://Shell';
import Meta from 'gi://Meta';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {dataFile} from './dataFiles.js';

/** Focus reporting and the Ctrl+Alt+P route hotkey. */
export class PenRouting {
    /** [ext] is the Extension object, needed for its GSettings (the keybinding). */
    constructor(ext) {
        this._ext = ext;
    }

    // --- Pen routing (ghost tracking) -------------------------------------------------------
    // The host cannot ask a Wayland compositor which app has focus, so this reports it: the focused
    // window's ids go to ~/.local/share/uscreen/focus.json, and Ctrl+Alt+P asks the host to flip the
    // pen between "graphics tablet" and "mouse" for that app (via ~/.local/share/uscreen/toggle_request).
    enable() {
        this._lastFocusJson = '';
        try {
            this._focusId = global.display.connect('notify::focus-window', () => this._writeFocus());
            this._writeFocus();
        } catch (e) {
            console.error(`[uscreen-app-lock] focus reporting unavailable: ${e}`);
        }
        try {
            this._keySettings = this._ext.getSettings();
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

    /** Disconnects the focus signal and removes the Ctrl+Alt+P keybinding. */
    disable() {
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

    /**
     * Writes the focused window's app id, window class and GTK id to `focus.json`. Skips menus and popups, so the app underneath keeps focus for routing, and skips writing when nothing changed.
     */
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
            dataFile('focus.json').replace_contents(
                new TextEncoder().encode(json), null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (e) {
            console.error(`[uscreen-app-lock] cannot write focus.json: ${e}`);
        }
    }

    /**
     * Asks the host to flip the pen route for the focused app, by writing the current time (microseconds) to `toggle_request`.
     */
    _toggleTablet() {
        try {
            dataFile('toggle_request').replace_contents(
                new TextEncoder().encode(`${GLib.get_real_time()}`), null, false,
                Gio.FileCreateFlags.REPLACE_DESTINATION, null);
        } catch (e) {
            console.error(`[uscreen-app-lock] cannot write toggle_request: ${e}`);
        }
    }
}
