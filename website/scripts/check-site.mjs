// scripts/check-site.mjs: checks the built site in dist/ and fails the build on a problem.
// Owns the checks: every internal link and anchor resolves, no third-party request is made by a page,
// flags such as `--fps` were not rewritten by smart punctuation, and no personal data (redact.mjs
// LEAK_PATTERNS) is in the pages. Uses no network. Does not check external links.

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ALLOWED_EMAILS, LEAK_PATTERNS } from './redact.mjs';

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const DIST = join(SITE, 'dist');
const BASE = (process.env.SITE_BASE ?? '/UScreen-Pro/').replace(/\/?$/, '/');
const problems = [];
const fail = (page, msg) => problems.push(`${page}: ${msg}`);

/** All files under [dir], recursively. */
function walk(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? walk(p) : [p];
  });
}

if (!existsSync(DIST)) {
  console.error('dist/ does not exist: run `astro build` first');
  process.exit(1);
}
const pages = walk(DIST).filter((f) => f.endsWith('.html') && !f.includes(`${join(DIST, 'api')}`));
const idCache = new Map();

/** The ids of an HTML file, cached. */
function idsOf(file) {
  if (!idCache.has(file)) idCache.set(file, new Set([...readFileSync(file, 'utf8').matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
  return idCache.get(file);
}

/** Maps a site URL path (starting with BASE) to a file in dist/, or null. */
function fileForUrl(path) {
  let rel = decodeURIComponent(path.slice(BASE.length));
  const candidates = [join(DIST, rel), join(DIST, rel, 'index.html'), join(DIST, `${rel}.html`)];
  return candidates.find((c) => existsSync(c) && statSync(c).isFile()) ?? null;
}

for (const page of pages) {
  const rel = page.slice(DIST.length + 1);
  const html = readFileSync(page, 'utf8');
  const pageUrl = BASE + rel.replace(/index\.html$/, '');
  for (const m of html.matchAll(/\s(?:href|src)="([^"]*)"/g)) {
    const target = m[1];
    if (!target || /^(mailto:|data:|javascript:)/.test(target)) continue;
    if (/^https?:\/\//.test(target)) {
      if (/\s(?:src)="/.test(m[0]) || /rel="stylesheet"[^>]*href="[^"]*"/.test(html.slice(m.index - 40, m.index + m[0].length + 20))) {
        if (!target.startsWith('https://github.com/')) fail(rel, `third-party request: ${target}`);
      }
      continue;
    }
    const [path, hash] = target.split('#');
    let abs;
    if (path === '') abs = pageUrl;
    else if (path.startsWith('/')) abs = path;
    else abs = new URL(path, `http://x${pageUrl}`).pathname;
    if (!abs.startsWith(BASE)) {
      fail(rel, `link outside the base path: ${target}`);
      continue;
    }
    const file = fileForUrl(abs);
    if (!file) {
      fail(rel, `broken link: ${target}`);
      continue;
    }
    if (hash && file.endsWith('.html') && !idsOf(file).has(decodeURIComponent(hash))) fail(rel, `missing anchor: ${target}`);
  }
  // A flag such as --fps must stay two ASCII hyphens: smart punctuation would turn it into an en dash.
  const text = html.replace(/<script[\s\S]*?<\/script>/g, '');
  const dashed = text.match(/[\s>(]–[a-z]{2,}/);
  if (dashed) fail(rel, `an en dash where a flag may have been: "${dashed[0].trim()}"`);
  for (const [kind, re] of Object.entries(LEAK_PATTERNS)) {
    const body = text.replace(/<wbr\s*\/?>/g, '').replace(/<[^>]+>/g, ' ');
    const hit = kind === 'e-mail address'
      ? [...body.matchAll(new RegExp(re.source, 'g'))].map((m) => m[0]).find((m) => !ALLOWED_EMAILS.has(m))
      : body.match(re)?.[0];
    if (hit) fail(rel, `personal data (${kind}): ${hit}`);
  }
}

if (problems.length) {
  console.error(`${problems.length} problem(s):\n${problems.slice(0, 80).join('\n')}${problems.length > 80 ? '\n...' : ''}`);
  process.exit(1);
}
console.log(`site check ok: ${pages.length} pages`);
