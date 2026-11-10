/*
 * tests/check-versions.mjs: checks that the Shell versions in metadata.json and in compat/versions.js are the same list.
 * Run with `node extensions/uscreen-app-lock/tests/check-versions.mjs`. It reads two files and loads no GNOME library,
 * so it runs anywhere. It does not test the extension itself (that needs a running Shell).
 */
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {dirname, join} from 'node:path';

import {SHELL_VERSIONS} from '../compat/versions.js';

const here = dirname(fileURLToPath(import.meta.url));
const metadata = JSON.parse(readFileSync(join(here, '..', 'metadata.json'), 'utf8'));

const declared = metadata['shell-version'].map(Number).sort((a, b) => a - b);
const known = Object.keys(SHELL_VERSIONS).map(Number).sort((a, b) => a - b);
let failed = false;

for (const v of declared) {
    if (!known.includes(v)) { console.error(`metadata.json lists Shell ${v}, compat/versions.js does not`); failed = true; }
}
for (const v of known) {
    if (!declared.includes(v)) { console.error(`compat/versions.js lists Shell ${v}, metadata.json does not`); failed = true; }
}
for (const [v, info] of Object.entries(SHELL_VERSIONS)) {
    if (!['tested', 'declared', 'unloadable'].includes(info.status)) { console.error(`Shell ${v}: unknown status '${info.status}'`); failed = true; }
}
if (failed) process.exit(1);
console.log(`ok: ${declared.length} Shell versions agree (${declared.join(', ')})`);
