/*
 * compat/versions.js: the GNOME Shell versions this extension names, and what is known about each.
 * Owns that table and nothing else; it is plain data (no gi:// imports) so tests/check-versions.mjs can read it.
 *
 * The list of versions GNOME Shell actually loads the extension on is `shell-version` in ../metadata.json.
 * tests/check-versions.mjs fails when the two lists differ, so a version cannot be added in one place only.
 *
 * To add a Shell version: add it to metadata.json and add a row here. Add a compat/shell-<n>.js only if
 * something behaves differently on that version (see ../../../docs/adding-support.md).
 */

/** `tested`: run by the maintainer. `declared`: named in metadata.json but never run. `unloadable`: named but cannot load. */
export const SHELL_VERSIONS = {
    42: {status: 'unloadable', note: 'extension.js is an ES module, the format of GNOME 45 and later; this version cannot load it'},
    43: {status: 'unloadable', note: 'extension.js is an ES module, the format of GNOME 45 and later; this version cannot load it'},
    44: {status: 'unloadable', note: 'extension.js is an ES module, the format of GNOME 45 and later; this version cannot load it'},
    45: {status: 'declared', note: 'not run by the maintainer'},
    46: {status: 'tested', note: 'Ubuntu 24.04 on Wayland'},
    47: {status: 'declared', note: 'not run by the maintainer'},
};

/** Text for the log line: how much is known about Shell major version [major]. */
export function describeShellVersion(major) {
    const v = SHELL_VERSIONS[major];
    return v ? `${v.status} (${v.note})` : 'not listed in compat/versions.js';
}
