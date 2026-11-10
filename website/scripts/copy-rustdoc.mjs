// scripts/copy-rustdoc.mjs: puts the rustdoc output of the host crate under dist/api/ so the site links to it.
// Owns the copy only. The documentation is built by `RUSTDOCFLAGS="--document-private-items" cargo doc --no-deps -p uscreen`
// (the host is a binary crate, so its items are private); without it this step prints a notice, and with
// REQUIRE_RUSTDOC=1 (CI) it fails.

import { cpSync, existsSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(SITE, '..', 'target', 'doc');
const target = join(SITE, 'dist', 'api');

if (!existsSync(join(source, 'uscreen', 'index.html'))) {
  const message = 'rustdoc output not found at target/doc/uscreen: run `RUSTDOCFLAGS="--document-private-items" cargo doc --no-deps -p uscreen`';
  if (process.env.REQUIRE_RUSTDOC === '1') {
    console.error(message);
    process.exit(1);
  }
  console.warn(`notice: ${message}; the API link will not resolve`);
  process.exit(0);
}
rmSync(target, { recursive: true, force: true });
cpSync(source, target, { recursive: true });
console.log('rustdoc copied to dist/api/');
