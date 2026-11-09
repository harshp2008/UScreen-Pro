/*
 * lib/contextProbe.js: the context probe for the pen's long-press menu.
 * Owns context_request (read) and context.json (written): where the pointer is, what is under it, and only the lengths of the selection and the clipboard, never the text.
 */

import Shell from 'gi://Shell';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {dataFile} from './dataFiles.js';

/** Answers context requests from the daemon. */
export class ContextProbe {
    // --- Context probe (pen long-press menu) --------------------------------------------------
    // The host writes ~/.local/share/uscreen/context_request when the pen was held still and lifted.
    // This answers in ~/.local/share/uscreen/context.json with what the menu needs to decide whether
    // (and what) to show: where the pointer is (a window, the panel/dock/shell UI, the desktop, the
    // overview), the window's frame, and how much text is highlighted / on the clipboard. Only
    // lengths are reported, never the text itself.
    enable() {
        this._ctxBusy = false;
        try {
            let file = dataFile('context_request');
            this._ctxMonitor = file.monitor_file(Gio.FileMonitorFlags.NONE, null);
            this._ctxMonitorId = this._ctxMonitor.connect('changed', (mon, f, other, ev) => {
                if (ev === Gio.FileMonitorEvent.CHANGES_DONE_HINT || ev === Gio.FileMonitorEvent.CREATED)
                    this._probeContext();
            });
        } catch (e) {
            console.error(`[uscreen-app-lock] context probe unavailable: ${e}`);
        }
    }

    /** Stops the `context_request` watcher and any pending probe timeout. */
    disable() {
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

    /**
     * Finds what is under screen position ([x], [y]) in pixels: zone `overview`, `window` (with its frame, whether it draws its own decorations, app id and window class), `shell` (panel, dock, popups) or `desktop` (the wallpaper layer). Returns `desktop` if the lookup fails.
     */
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

    /**
     * Answers a context request: writes `context.json` with the pointer position, the zone, and the lengths (never the text) of the primary selection and the clipboard. A clipboard owner that does not answer within 150 ms is written as length 0, so the host never waits on it.
     */
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
                dataFile('context.json').replace_contents(
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
}
