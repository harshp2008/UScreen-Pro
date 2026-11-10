// scripts/convert-experiments.mjs: converts experiments_and_findings/*.md into the record format of the site.
// Owns the mapping from a free-form write-up to the fixed record layout (an opening paragraph, a fields table,
// then What was tested, Result, Not verified and Sources), the experiment index page and the redaction of the
// output. Does not own the raw evidence (screenshots, logs): those stay in experiments_and_findings/ and are
// never published.
//
// Run by hand: `node scripts/convert-experiments.mjs`. The generated pages are committed, so a person can read and
// correct them; running the script again overwrites them. It never invents a result, a date or a build: a field
// the write-up does not state is written "not recorded", and every sentence in a record is a sentence of the
// original (headings are moved under the three sections, nothing is rewritten).

import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { redact, redactionSummary } from './redact.mjs';

const SITE = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const REPO = resolve(SITE, '..');
const SRC = join(REPO, 'experiments_and_findings');
const OUT = join(SITE, 'src/content/docs/experiments');
const BASE = (process.env.SITE_BASE ?? '/UScreen-Pro/').replace(/\/?$/, '/');
const BLOB = 'https://github.com/harshp2008/UScreen-Pro/blob/main/';

/** Heading words that start the "Not verified" section. */
const NOT_VERIFIED = /not verified|not proven|could not verify|could not be verified|unverified|caveat|limitation|open (question|item)|what is not|not tested|what i could not|not (yet )?(measured|built)/i;
/** Heading words that start the "Result" section: what was found, changed or decided. */
const RESULT = /result|outcome|conclusion|verification|verdict|finding|proof|measurement|empirical|decision|resolution|remediation|fix|what changed|changes?$|what was built|implementation|deviation|summary of changes|patch|solution|change log/i;

/** URL-safe slug of a title: lower case, runs of other characters become one hyphen. */
const slugify = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 70).replace(/-+$/, '');

/** Cleans a first heading into a title: drops the "Experiment & Findings:" label and a leading number. */
function cleanTitle(raw, file) {
  let t = (raw ?? '').replace(/\*\*/g, '').trim();
  // Labels the write-ups put in front of the real title, with or without a number: "Lab Notebook:", "Experiment & Finding 16:", "Finding 09 -".
  t = t.replace(/^(Lab Notebook|Experiment\s*(&|and)\s*Findings?|Experiment|Finding|Fix)(\s+\d+[a-z]?)?\s*[:\-—–.]\s*/i, '');
  t = t.replace(/^\d+[a-z]?\s*[:\-—–.]\s*/, '');
  if (!t) t = file.replace(/^\d+_/, '').replace(/\.md$/, '').replace(/_/g, ' ');
  t = t[0].toUpperCase() + t.slice(1);
  return t.length > 90 ? t.slice(0, 87).replace(/\s+\S*$/, '') + '...' : t;
}

/** First sentence-ish chunk of [text], at most [max] characters, without Markdown noise. */
function firstSentence(text, max = 170) {
  const t = text.replace(/\s+/g, ' ').replace(/\*\*|`/g, '').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').trim();
  const end = t.slice(30).search(/(?<!\d)[.!?](\s|$)/);
  if (end >= 0 && end + 31 <= max + 60) return t.slice(0, end + 31).trim();
  if (t.length <= max) return t;
  return t.slice(0, max).replace(/\s+\S*$/, '') + ' ...';
}

/** Splits a document body into [{level, title, body}] sections by its headings (code fences respected). */
function sections(text) {
  const out = [{ level: 0, title: '', lines: [] }];
  let fence = false;
  for (const line of text.split('\n')) {
    if (/^```/.test(line)) fence = !fence;
    const h = !fence && line.match(/^(#{1,6})\s+(.*)$/);
    if (h) out.push({ level: h[1].length, title: h[2].trim(), lines: [] });
    else out[out.length - 1].lines.push(line);
  }
  return out.map((s) => ({ ...s, body: s.lines.join('\n').trim() }));
}


/** Commit hashes the write-up gives as a build: seven hex digits with `-dirty`, or after the word build, commit or binary. */
function buildsIn(raw) {
  const found = new Set();
  for (const m of raw.matchAll(/\b([0-9a-f]{7})(-dirty)?\b/g)) {
    const h = m[1] + (m[2] ?? '');
    if (!/\d/.test(m[1]) || !/[a-f]/.test(m[1])) continue;
    const before = raw.slice(Math.max(0, m.index - 60), m.index);
    if (m[2] || /build|commit|binary|version/i.test(before)) found.add(h);
  }
  return [...found].slice(0, 4);
}

/** The files of the evidence folders that belong to experiment [id] (kept in the repository, not published). */
function evidenceFor(id) {
  const dirs = [];
  const num = id.replace(/[a-z]$/, '');
  for (const d of readdirSync(SRC, { withFileTypes: true })) {
    if (!d.isDirectory() || d.name === '__pycache__') continue;
    if (d.name === num || d.name.startsWith(`${num}_`)) dirs.push(d.name);
  }
  if (existsSync(join(SRC, 'screenshots', num))) dirs.push(`screenshots/${num}`);
  return dirs;
}

/** Turns the original's links and images into text that is safe to publish. */
function cleanBody(text) {
  return text
    // Images are evidence; they stay in the repository and are not published.
    .replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_, alt, p) => `*(image not published: ${p.split('/').pop()}${alt ? ` - ${alt}` : ''})*`)
    // Links to files in the repository become GitHub links; links to other experiment files become plain text.
    .replace(/\]\((?!https?:|#|mailto:)([^)\s]+)\)/g, (_, p) => {
      const clean = p.replace(/^\.\//, '').replace(/^\.\.\//, '');
      return /^\d+_.*\.md$/.test(clean) ? `](${BASE}experiments/)` : `](${BLOB}${clean.startsWith('docs') || clean.startsWith('host') ? clean : 'experiments_and_findings/' + clean})`;
    });
}

/** Renders one section as `### title` plus body (the first heading level is lowered to three). */
const render = (s) => `${s.title ? `### ${s.title.replace(/^\d+[.)]\s*/, '')}\n\n` : ''}${cleanBody(s.body)}\n`;

const files = readdirSync(SRC).filter((f) => /^\d+.*\.md$/.test(f)).sort((a, b) => parseInt(a) - parseInt(b) || a.localeCompare(b));
const numbers = files.map((f) => f.match(/^(\d+)/)[1]);
const records = [];

rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });

files.forEach((file, i) => {
  const num = numbers[i];
  const dup = numbers.filter((n) => n === num).length > 1;
  const suffix = dup ? 'ab'[numbers.slice(0, i).filter((n) => n === num).length] : '';
  const id = `${num.replace(/^0+(?=\d)/, '')}${suffix}`;
  const raw = readFileSync(join(SRC, file), 'utf8');
  const secs = sections(raw);
  const firstH1 = secs.find((s) => s.level === 1);
  const title = cleanTitle(firstH1?.title, file);
  const rest = secs.filter((s) => s !== firstH1);

  // Opening paragraph: the first paragraph before any sub-heading, else the first paragraph of the first section.
  const preamble = [firstH1?.body ?? '', rest[0]?.level === 0 ? rest[0].body : ''].join('\n').trim();
  const pool = preamble || rest.find((s) => s.body)?.body || '';
  const firstPara = pool.split(/\n\s*\n/).find((p) => p.trim() && !/^\s*(```|\||[-*]\s|\d+\.\s)/.test(p)) ?? title;
  const lead = cleanBody(firstPara.trim());

  // Distribute the sections.
  const tested = [];
  const result = [];
  const notVerified = [];
  let seenResult = false;
  for (const s of rest) {
    if (s.level === 0 && !s.title) {
      if (s.body && s.body !== preamble) tested.push({ ...s, title: '' });
      continue;
    }
    if (NOT_VERIFIED.test(s.title)) notVerified.push(s);
    else if (RESULT.test(s.title)) { seenResult = true; result.push(s); }
    else (seenResult ? result : tested).push(s);
  }
  if (preamble && preamble !== firstPara.trim()) tested.unshift({ level: 0, title: '', body: preamble.replace(firstPara.trim(), '').trim() });

  // Fields: only what the write-up states.
  const dates = [...new Set([...raw.slice(0, 1500).matchAll(/\b(20\d{2}-\d{2}-\d{2})\b/g)].map((m) => m[1]))];
  const builds = buildsIn(raw);
  const verdictSec = rest.find((s) => /verdict|conclusion|decision|outcome/i.test(s.title) && s.body);
  const verdictLine = raw.match(/^\**(?:Verdict|Decision|Conclusion|Outcome)\**\s*[:\-—]\s*(.+)$/im)?.[1];
  const verdict = verdictLine ? firstSentence(verdictLine) : verdictSec ? firstSentence(verdictSec.body.split(/\n\s*\n/)[0]) : 'not recorded';
  const evidence = evidenceFor(num);
  const rawResults = evidence.length
    ? `Kept in the repository under ${evidence.map((d) => `\`experiments_and_findings/${d}/\``).join(', ')}; not published.`
    : 'not recorded';

  const slug = `${id}-${slugify(title)}`;
  const fields = [
    ['ID', `${id}${suffix ? '' : ''}`],
    ['Date', dates.length ? dates.join(', ') + ' (as written in the file)' : 'not recorded'],
    ['Build', builds.length ? builds.map((b) => `\`${b}\``).join(', ') + ' (as written in the file)' : 'not recorded'],
    ['Verdict', verdict],
    ['Raw results', rawResults],
  ];
  const parts = [
    lead,
    '',
    '| Field | Value |\n| :-- | :-- |\n' + fields.map(([k, v]) => `| **${k}** | ${v.replace(/\|/g, '\\|')} |`).join('\n'),
    '',
    '## What was tested',
    '',
    tested.length ? tested.map(render).join('\n') : 'The write-up does not separate a question or a setup from its result; it is under Result.',
    '',
    '## Result',
    '',
    result.length ? result.map(render).join('\n') : 'not recorded as a separate section; see What was tested.',
    '',
    '## Not verified',
    '',
    notVerified.length ? notVerified.map((s) => cleanBody(s.body)).join('\n\n') : 'The write-up lists nothing as not verified. That is not a claim that everything was.',
    '',
    '## Sources',
    '',
    `- [\`experiments_and_findings/${file}\`](${BLOB}experiments_and_findings/${file})`,
  ];
  const body = redact(parts.join('\n'), file);
  const fm = `---\ntitle: ${JSON.stringify(`${id} ${title}`)}\nsidebar:\n  order: ${i + 1}\n  label: ${JSON.stringify(`${id} ${title}`.slice(0, 60))}\n---\n\n`;
  writeFileSync(join(OUT, `${slug}.md`), fm + body.trim() + '\n');
  records.push({ id, title, slug, verdict: redact(verdict), num: parseInt(num) });
});

// The index: grouped by ranges of IDs, because the write-ups carry no dates.
const GROUPS = [
  [1, 8, 'Records 1 to 8: the first bugs and the Android immersive mode'],
  [14, 19, 'Records 14 to 19: encoder, pacing and latency tuning'],
  [20, 29, 'Records 20 to 29: several tablets, connectivity and deployment'],
  [33, 50, 'Records 33 to 50: the desktop app redesign and polish'],
  [51, 66, 'Records 51 to 66: transport, Wi-Fi setup, the EVDI freeze and power'],
  [69, 76, 'Records 69 to 76: the input engine, the touchpad and stylus research'],
  [81, 103, 'Records 81 to 103: stylus handling, ghost tracking and the pen menu'],
];
const cell = (s) => s.replace(/\|/g, '\\|');
let index = `---\ntitle: "Experiments"\nsidebar:\n  order: 0\n  label: "Index"\n---\n\nOne record for each write-up in \`experiments_and_findings/\`. The write-ups are working notes kept while the code was changed, not lab reports: some record a measurement, many record a bug, a fix and how it was checked. The pages keep the sentences of the original and move them into one layout.\n\nEach record opens with a short paragraph and a table (ID, date, build, verdict, raw results), then has the sections What was tested, Result, Not verified and Sources. A field the write-up does not state is written "not recorded". Nothing was re-run to write these pages.\n\n**Dates.** Most write-ups carry no date, so the groups below are ranges of IDs, not dates. Where a date is in the write-up it is in the record.\n\n**Build.** The build is the commit hash, with \`-dirty\` when the tree had uncommitted changes, exactly as the write-up names it. No version is inferred from a hash.\n\n**Verdict.** The first sentence of the write-up's own verdict, conclusion, decision or outcome. "not recorded" means the write-up has none under those names; read its Result.\n\n**Raw results.** Screenshots, logs and scripts are kept in the repository and are not published here. Personal data (home paths, device serials, addresses) was removed from the pages.\n\n`;
for (const [lo, hi, label] of GROUPS) {
  const rows = records.filter((r) => r.num >= lo && r.num <= hi);
  if (!rows.length) continue;
  index += `## ${label}\n\n| ID | Experiment | Verdict |\n| :--- | :--- | :--- |\n${rows.map((r) => `| [${r.id}](${BASE}experiments/${r.slug}/) | ${cell(r.title)} | ${cell(r.verdict)} |`).join('\n')}\n\n`;
}
index += `## Sources\n\n- \`experiments_and_findings/\` (the write-ups and their evidence folders)\n- \`website/scripts/convert-experiments.mjs\` (the conversion)\n`;
writeFileSync(join(OUT, 'index.md'), redact(index, 'index'));
console.log(`${records.length} records written; redaction: ${redactionSummary()}`);
