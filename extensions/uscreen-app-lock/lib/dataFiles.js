/*
 * lib/dataFiles.js: the shared directory ~/.local/share/uscreen/, the whole interface between this extension and the UScreen daemon.
 * Owns where files live and how they are written; what each file means is documented in the module that uses it.
 */
// PLATFORM(gnome): runs inside GNOME Shell; the supported Shell versions are listed in compat/versions.js and metadata.json

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

/**
 * The file `name` in `~/.local/share/uscreen/`, creating the directory if needed. This directory is the whole interface to the host daemon.
 */
export function dataFile(name) {
    let dir = GLib.build_filenamev([GLib.get_home_dir(), '.local', 'share', 'uscreen']);
    GLib.mkdir_with_parents(dir, 0o755);
    return Gio.File.new_for_path(GLib.build_filenamev([dir, name]));
}

/** Writes [text] to the file [name] in the data directory, logging a failure. */
export function writeFile(name, text) {
    try {
        dataFile(name).replace_contents(
            new TextEncoder().encode(text), null, false, Gio.FileCreateFlags.REPLACE_DESTINATION, null);
    } catch (e) {
        console.error(`[uscreen-app-lock] cannot write ${name}: ${e}`);
    }
}
