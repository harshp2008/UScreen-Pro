// scripts/generate.mjs: writes the pages of the site that are generated from the repository.
// Owns: the Project documents (mirrored from docs/), the Release notes and the changelog page (from
// CHANGELOG.md), the CLI reference (from `uscreen --help`), and the configuration page (from the config
// structs in host/src/config/). Does not own the hand-written pages or the experiment records, which are
// committed under src/content/docs/.
//
// Everything written here is git-ignored (see website/.gitignore). Every text passes through redact() on
// its way out. `npm run generate` runs this; `npm run build` runs it first.

import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { redact, redactionSummary } from './redact.mjs';

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = resolve(SITE, '..');
const DOCS = join(SITE, 'src/content/docs');
const BASE = (process.env.SITE_BASE ?? '/UScreen-Pro/').replace(/\/?$/, '/');
const GITHUB = 'https://github.com/harshp2008/UScreen-Pro';
const BLOB = `${GITHUB}/blob/main/`;

/** Reads a repository file as text. */
const read = (rel) => readFileSync(join(REPO, rel), 'utf8');

/** Writes a generated page (creating its folder) after redaction. */
function page(rel, front, body) {
  const file = join(DOCS, rel);
  mkdirSync(dirname(file), { recursive: true });
  const fm = Object.entries(front)
    .map(([k, v]) => (typeof v === 'object' ? `${k}:\n${Object.entries(v).map(([a, b]) => `  ${a}: ${JSON.stringify(b)}`).join('\n')}` : `${k}: ${JSON.stringify(v)}`))
    .join('\n');
  writeFileSync(file, `---\n${fm}\n---\n\n${redact(body, rel).trim()}\n`);
}

/** Clears a generated folder so a removed source leaves no stale page. */
function clean(rel) {
  rmSync(join(DOCS, rel), { recursive: true, force: true });
}

/** Strips the leading HTML comment header that every repository document starts with. */
const stripHeader = (text) => text.replace(/^<!--[\s\S]*?-->\s*/, '');

/** Splits the first `# ` heading off a Markdown text: [title, rest]. */
function splitTitle(text) {
  const m = text.match(/^#\s+(.+)\n/m);
  if (!m) return ['', text];
  return [m[1].trim(), text.replace(m[0], '')];
}

// ---------------------------------------------------------------------------------------------
// Project documents: docs/*.md mirrored under project/, so docs/ stays the only copy.
// ---------------------------------------------------------------------------------------------

/** file (relative to the repository), page name, sidebar order, sidebar label */
const MIRRORED = [
  ['docs/development.md', 'code-layout', 1, 'Code layout and building'],
  ['docs/architecture.md', 'behavior-notes', 2, 'Behaviour notes (architecture and protocol)'],
  ['docs/desktop-backends.md', 'design-desktop-backends', 3, 'Design: desktop backends'],
  ['docs/installer.md', 'design-installer', 4, 'Design: the installer'],
  ['docs/HEVC_VAAPI_PATCH.md', 'design-hevc-vaapi', 5, 'Design: HEVC VA-API'],
  ['docs/adding-support.md', 'adding-support', 6, 'Adding a distro, a desktop or a GNOME version'],
  ['docs/comment-style.md', 'comment-style', 7, 'Comment style'],
  ['docs/benchmarks.md', 'benchmarks', 8, 'Benchmarks'],
  ['docs/compatibility.md', 'compatibility', 9, 'Compatibility'],
  ['docs/faq.md', 'knowledge', 10, 'Knowledge base (FAQ)'],
  ['docs/backlog.md', 'backlog', 11, 'Backlog'],
  ['docs/codebase-changes.md', 'history', 12, 'Project history: the cleanup branch'],
  ['docs/file-headers.md', 'file-headers', 13, 'Files without a header comment'],
  ['SECURITY.md', 'security', 14, 'Security'],
  ['THIRD_PARTY_LICENSES.md', 'third-party', 15, 'Third-party components'],
  ['docs/installation.md', 'installation-notes', 16, 'Installation (long form)'],
  ['docs/troubleshooting.md', 'troubleshooting-notes', 17, 'Troubleshooting (long form)'],
];

const mirroredBySource = new Map(MIRRORED.map(([f, slug]) => [f, slug]));

/** Rewrites the links of a mirrored document so they work on the site or on GitHub. */
function fixLinks(text, fromFile) {
  const dir = dirname(fromFile);
  return text.replace(/\]\(([^)\s]+)\)/g, (whole, target) => {
    if (/^(https?:|mailto:|#)/.test(target)) return whole;
    const [path, hash = ''] = target.split('#');
    const abs = join(dir, path).replace(/\\/g, '/');
    const slug = mirroredBySource.get(abs);
    if (slug) return `](${BASE}project/${slug}/${hash ? '#' + hash : ''})`;
    if (abs === 'CHANGELOG.md') return `](${BASE}changelog/)`;
    return `](${BLOB}${abs}${hash ? '#' + hash : ''})`;
  });
}

function mirrorProjectDocs() {
  for (const f of readdirSync(join(DOCS, 'project'))) if (f !== 'index.md') rmSync(join(DOCS, 'project', f), { force: true });
  for (const [file, slug, order, label] of MIRRORED) {
    const [title, body] = splitTitle(stripHeader(read(file)));
    page(`project/${slug}.md`, { title: title || label, sidebar: { order, label } },
      `${fixLinks(body, file)}\n\n## Sources\n\n- [\`${file}\`](${BLOB}${file})\n`);
  }
}

// ---------------------------------------------------------------------------------------------
// Release notes and the changelog page, from CHANGELOG.md.
// ---------------------------------------------------------------------------------------------

function releaseNotes() {
  clean('release-notes');
  const text = stripHeader(read('CHANGELOG.md'));
  const parts = text.split(/^## /m);
  const intro = parts.shift().replace(/^#\s+Changelog\s*/m, '').trim();
  const rows = [];
  let order = 1;
  for (const part of parts) {
    const nl = part.indexOf('\n');
    const heading = part.slice(0, nl).trim();
    const body = part.slice(nl + 1).trim();
    const m = heading.match(/^([\w.\-]+)(?:\s+[—-]\s+(\d{4}-\d{2}-\d{2}))?/);
    const version = m ? m[1] : heading;
    const date = m && m[2] ? m[2] : 'not released';
    const slug = version.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    rows.push({ version, date, slug });
    page(`release-notes/${slug}.md`, { title: version === 'Unreleased' ? 'Unreleased' : version, sidebar: { order: order++ } },
      `Released: ${date}.\n\n${fixLinks(body, 'CHANGELOG.md')}\n\n## Sources\n\n- [\`CHANGELOG.md\`](${BLOB}CHANGELOG.md), section "${heading}"\n`);
  }
  page('release-notes/index.md', { title: 'Release notes', sidebar: { order: 0, label: 'Overview' } },
    `One page for each section of the changelog, newest first. ${intro.replace(/\n/g, ' ')}\n\n| Version | Date |\n| :--- | :--- |\n${rows.map((r) => `| [${r.version}](${BASE}release-notes/${r.slug}/) | ${r.date} |`).join('\n')}\n\n## Sources\n\n- [\`CHANGELOG.md\`](${BLOB}CHANGELOG.md)\n`);
  page('changelog.md', { title: 'Changelog' },
    `The full changelog is [\`CHANGELOG.md\`](${BLOB}CHANGELOG.md). The same sections are published as [release notes](${BASE}release-notes/), one page each.\n\n## Sources\n\n- [\`CHANGELOG.md\`](${BLOB}CHANGELOG.md)\n`);
}

// ---------------------------------------------------------------------------------------------
// CLI reference, from the --help of the built daemon.
// ---------------------------------------------------------------------------------------------

/** The built `uscreen` binary: $USCREEN_BIN, else target/release, else target/debug. */
function uscreenBinary() {
  const candidates = [process.env.USCREEN_BIN, join(REPO, 'target/release/uscreen'), join(REPO, 'target/debug/uscreen')].filter(Boolean);
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error('No built uscreen binary: run `cargo build -p uscreen` first (or set USCREEN_BIN).');
  return found;
}

/** Output of `uscreen <args> --help`. The binary only prints text for --help; it starts nothing. */
const help = (bin, args) => execFileSync(bin, [...args, '--help'], { encoding: 'utf8', env: { PATH: process.env.PATH ?? '' } }).trimEnd();

function cliReference() {
  clean('reference');
  const bin = uscreenBinary();
  const root = help(bin, []);
  const names = [...root.matchAll(/^  ([a-z][a-z-]*)\s{2,}(.*)$/gm)].map((m) => [m[1], m[2]]).filter(([n]) => n !== 'help');
  page('reference/index.md', { title: 'CLI reference', sidebar: { order: 0, label: 'Overview' } },
    `The output of \`--help\`, taken from the built binary when the site is built, so it cannot drift from the code.\n\n| Command | What it does |\n| :--- | :--- |\n${names.map(([n, d]) => `| [\`uscreen ${n}\`](${BASE}reference/uscreen-${n}/) | ${d.trim()} |`).join('\n')}\n\nThe options before the command (\`--encoder\`, \`--fps\`, ...) are on the page [uscreen](${BASE}reference/uscreen/).\n\n## Sources\n\n- \`uscreen --help\`, built from [\`host/src/cli/mod.rs\`](${BLOB}host/src/cli/mod.rs)\n`);
  page('reference/uscreen.md', { title: 'uscreen', sidebar: { order: 1 } },
    `\`\`\`text\n${root}\n\`\`\`\n\n## Sources\n\n- \`uscreen --help\`, built from [\`host/src/cli/mod.rs\`](${BLOB}host/src/cli/mod.rs)\n`);
  names.forEach(([n], i) => {
    page(`reference/uscreen-${n}.md`, { title: `uscreen ${n}`, sidebar: { order: i + 2 } },
      `\`\`\`text\n${help(bin, [n])}\n\`\`\`\n\n## Sources\n\n- \`uscreen ${n} --help\`, built from [\`host/src/cli/mod.rs\`](${BLOB}host/src/cli/mod.rs)\n`);
  });
}

// ---------------------------------------------------------------------------------------------
// Configuration page, from the config structs.
// ---------------------------------------------------------------------------------------------

/** Parses `pub struct <name> { ... }` into [{name, type, doc}] in source order. */
function structFields(source, name) {
  const start = source.indexOf(`pub struct ${name} {`);
  if (start < 0) throw new Error(`struct ${name} not found`);
  const end = source.indexOf('\n}\n', start);
  const lines = source.slice(start, end).split('\n').slice(1);
  const out = [];
  let doc = [];
  let attrs = [];
  for (const line of lines) {
    const t = line.trim();
    if (t.startsWith('///')) doc.push(t.replace(/^\/\/\/ ?/, ''));
    else if (t.startsWith('#[')) attrs.push(t);
    else {
      const m = t.match(/^pub (\w+): (.+),$/);
      if (m) out.push({ name: m[1], type: m[2], doc: doc.join(' ').replace(/\s+/g, ' ').trim(), attrs });
      doc = [];
      attrs = [];
    }
  }
  return out;
}

/** Parses the `Self { field: value, ... }` of `impl Default for <name>` into a map. */
function defaults(source, name) {
  const start = source.indexOf(`impl Default for ${name}`);
  if (start < 0) return new Map();
  const end = source.indexOf('\n}\n', start);
  const map = new Map();
  for (const m of source.slice(start, end).matchAll(/^\s{12}(\w+): (.+),$/gm)) map.set(m[1], m[2].replace(/\.into\(\)$/, '').replace(/String::new\(\)/, '""').replace(/Vec::new\(\)|vec!\[\]/, '[]'));
  return map;
}

function configurationPage() {
  const globalSrc = read('host/src/config/mod.rs');
  const profileSrc = read('host/src/config/profile.rs');
  const g = structFields(globalSrc, 'FileConfig');
  const gd = defaults(globalSrc, 'FileConfig');
  const p = structFields(profileSrc, 'TabletProfile');
  const row = (f, def) => `| \`${f.name}\` | \`${f.type.replace(/\|/g, '\\|')}\` | ${def} | ${f.doc.replace(/\|/g, '\\|') || 'not documented in the source'} |`;
  const globalRows = g.filter((f) => f.name !== 'tablets').map((f) => row(f, gd.has(f.name) ? `\`${gd.get(f.name)}\`` : 'see source'));
  const profileRows = p.map((f) => row(f, f.type.startsWith('Option<') ? 'unset (inherits the global value)' : '`0` / empty'));
  page('configuration.md', { title: 'Configuration' },
    `UScreen reads \`~/.config/uscreen/config.toml\`. The desktop app and the Android app edit the same file, so keys you add by hand are kept (unknown keys are preserved when the app saves). Every key has a default: an empty file is a valid file. Values outside their range are clamped when the file is loaded.\n\n## Global settings\n\nThe table is read from the source of the settings struct when the site is built. A default shown as "see source" is computed in code.\n\n| Key | Type | Default | Meaning |\n| :--- | :--- | :--- | :--- |\n${globalRows.join('\n')}\n\n## Per-device settings\n\nA device has its own section, \`[tablets.<serial>]\`. A key set there overrides the global one for that device only; a key that is not set means "use the global value". The "last change wins" rule between the two is explained in [Settings and protocol](${BASE}settings-and-protocol/).\n\n| Key | Type | Default | Meaning |\n| :--- | :--- | :--- | :--- |\n${profileRows.join('\n')}\n\n## Not verified\n\nThe defaults of fields that are computed in code are not shown. Values were not checked against a running daemon.\n\n## Sources\n\n- [\`host/src/config/mod.rs\`](${BLOB}host/src/config/mod.rs) (\`FileConfig\` and its \`Default\`)\n- [\`host/src/config/profile.rs\`](${BLOB}host/src/config/profile.rs) (\`TabletProfile\`)\n- [\`host/src/config/limits.rs\`](${BLOB}host/src/config/limits.rs) (the allowed ranges)\n`);
}

mkdirSync(join(DOCS, 'project'), { recursive: true });
mirrorProjectDocs();
releaseNotes();
cliReference();
configurationPage();
console.log('generated:', redactionSummary());
