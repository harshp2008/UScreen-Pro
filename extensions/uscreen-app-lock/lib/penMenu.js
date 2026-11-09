/*
 * lib/penMenu.js: the pen's long-press menu, drawn inside the Shell so it is not a window (nothing in the dock, no focus change).
 * Owns menu_request.json and menu_close (read) and menu_shown and menu_result (written). Its look is in ../stylesheet.css.
 */

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {dataFile, writeFile} from './dataFiles.js';

/** The pen menu shown on request. */
export class PenMenu {
    // --- Pen menu -----------------------------------------------------------------------------
    // The host writes ~/.local/share/uscreen/menu_request.json ({id, items, style, timeout_ms}); the menu
    // is drawn here as Shell chrome at the pointer, `menu_shown` gets the id as soon as it is up, and
    // `menu_result` gets {id, action} when a button is tapped ("none" if dismissed or timed out).
    enable() {
        this._menuBox = null;
        this._menuId = null;
        this._menuCapture = null;
        this._menuTimeout = null;
        try {
            // The host asks for the menu to close when a tap elsewhere ended (the shell cannot see
            // taps that go to app windows).
            this._closeMonitor = dataFile('menu_close').monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._closeMonitorId = this._closeMonitor.connect('changed', (mon, f, other, ev) => {
                if (ev !== Gio.FileMonitorEvent.CHANGES_DONE_HINT && ev !== Gio.FileMonitorEvent.CREATED)
                    return;
                try {
                    let [ok, bytes] = f.load_contents(null);
                    this._finishMenu(Number(new TextDecoder().decode(bytes).trim()), 'none');
                } catch (e) {}
            });
            let file = dataFile('menu_request.json');
            this._menuMonitor = file.monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._menuMonitorId = this._menuMonitor.connect('changed', (mon, f, other, ev) => {
                if (ev === Gio.FileMonitorEvent.CHANGES_DONE_HINT || ev === Gio.FileMonitorEvent.CREATED)
                    this._onMenuRequest();
            });
        } catch (e) {
            console.error(`[uscreen-app-lock] pen menu unavailable: ${e}`);
        }
    }

    /** Removes any menu on screen and stops the menu request and close watchers. */
    disable() {
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

    /**
     * Reads `menu_request.json` and shows the menu it describes. Ignored when unreadable, empty, or the same request id as the menu already shown.
     */
    _onMenuRequest() {
        let req;
        try {
            let [ok, bytes] = dataFile('menu_request.json').load_contents(null);
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

    /**
     * Builds the menu (text or icon buttons) as Shell top chrome above the pointer, or below it when there is no room above, kept inside the work area. A tap outside it, or the request's timeout (8 s by default), dismisses it. Writes the request id to `menu_shown` once it is on screen.
     */
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
        writeFile('menu_shown', `${req.id}`);
    }

    /**
     * Ends the menu with [id] by removing it and writing `{id, action}` to `menu_result`; `none` means dismissed. Does nothing if [id] is not the menu on screen.
     */
    _finishMenu(id, action) {
        if (!this._menuBox || this._menuId !== id) return;
        this._removeMenu();
        writeFile('menu_result', JSON.stringify({id, action}) + '\n');
    }

    /** Takes the menu off the screen and cancels its timeout and tap-outside handler. */
    _removeMenu() {
        if (this._menuTimeout) { GLib.source_remove(this._menuTimeout); this._menuTimeout = null; }
        if (this._menuCapture) { global.stage.disconnect(this._menuCapture); this._menuCapture = null; }
        if (this._menuBox) {
            Main.layoutManager.removeChrome(this._menuBox);
            this._menuBox.destroy();
            this._menuBox = null;
        }
    }
}
