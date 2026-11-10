/*
 * compat/index.js: the entry point of the Shell-version compatibility layer.
 * Owns detecting the running Shell's major version; the table of known versions is in versions.js and the
 * version-dependent monitor lookup in monitors.js. Other modules import from here, never from `Config` directly.
 */

import {PACKAGE_VERSION} from 'resource:///org/gnome/shell/misc/config.js';

import {describeShellVersion} from './versions.js';

export {monitorIndexForConnector} from './monitors.js';

/** The running GNOME Shell's major version as a number (for example 46), or 0 if it cannot be read. */
export function shellMajor() {
    const major = parseInt(String(PACKAGE_VERSION).split('.')[0], 10);
    return Number.isFinite(major) ? major : 0;
}

/** One line saying which Shell this is and what is known about it, for the log. */
export function describeShell() {
    return `GNOME Shell ${PACKAGE_VERSION}: ${describeShellVersion(shellMajor())}`;
}
