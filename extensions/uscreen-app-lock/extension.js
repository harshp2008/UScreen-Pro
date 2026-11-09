/*
 * UScreen GNOME Shell extension (runs inside gnome-shell, GNOME 45+ module format).
 *
 * Four jobs for the UScreen daemon, which cannot ask a Wayland compositor for any of this. Each is one module
 * in lib/; this file only starts and stops them:
 *   1. App lock (lib/appLock.js): keep chosen apps on the virtual display (config in app_lock.json).
 *   2. Pen routing (lib/penRouting.js): report the focused app (focus.json) and the Ctrl+Alt+P route hotkey
 *      (toggle_request), so the pen can be a graphics tablet in Krita and a mouse in Chrome.
 *   3. Context probe (lib/contextProbe.js): on request (context_request) report where the pointer is and how much
 *      text is selected / on the clipboard (context.json), for the pen's long-press menu.
 *   4. Pen menu (lib/penMenu.js): draw the long-press menu (cut / copy / paste / ...) inside the Shell on request
 *      (menu_request.json), so it is not a window: nothing in the dock, no focus change. The answer goes to
 *      menu_shown (on screen) and menu_result (the choice).
 * All files live in ~/.local/share/uscreen/ (lib/dataFiles.js). Differences between GNOME Shell versions are
 * confined to compat/.
 */
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {describeShell} from './compat/index.js';
import {AppLock} from './lib/appLock.js';
import {PenRouting} from './lib/penRouting.js';
import {ContextProbe} from './lib/contextProbe.js';
import {PenMenu} from './lib/penMenu.js';

export default class UScreenAppLockExtension extends Extension {
    /** Called by GNOME Shell when the extension is switched on: starts the four modules. `disable()` stops them. */
    enable() {
        this._appLock = new AppLock();
        this._penRouting = new PenRouting(this);
        this._contextProbe = new ContextProbe();
        this._penMenu = new PenMenu();

        this._appLock.enable();
        this._penRouting.enable();
        this._contextProbe.enable();
        this._penMenu.enable();
        console.log(`[uscreen-app-lock] Extension enabled on ${describeShell()}`);
    }

    /** Called when the extension is switched off or the session ends: stops every module, in reverse order. */
    disable() {
        this._penMenu.disable();
        this._contextProbe.disable();
        this._penRouting.disable();
        this._appLock.disable();
        this._penMenu = null;
        this._contextProbe = null;
        this._penRouting = null;
        this._appLock = null;
        console.log('[uscreen-app-lock] Extension disabled');
    }
}
