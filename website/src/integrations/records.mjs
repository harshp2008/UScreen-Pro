// Records: a table whose cells are paragraphs, built as one card per row. A table with a long cell
// is unreadable as a table in a 45rem column (the cells get 25 characters each), so this
// integration rewrites such a <table> in the built HTML as
//
//   <ul class="records [records-stack] [records-sentence]" role="list">
//     <li class="record">
//       <div class="record-key">first column</div>
//       <dl class="record-fields">
//         <div class="field field-short"><dt>header</dt><dd>cell</dd></div>
//         <div class="field field-long"><dt>header</dt><dd>cell</dd></div> ...
//       </dl>
//     </li> ...
//   </ul>
//
// A list whose keys all start with a whole number (1, `2 (fixed in step 1)`, `12 files`) is a list
// of steps: an <ol class="records records-steps"> whose key is
// `<span class="record-num">2</span><span class="record-note">fixed in step 1</span>`.
//
// A list whose longest field (a cell outside the key column) has more than TITLED_MIN characters of
// text is a list of titled cards: the key is the title at the top of the card, then the short fields
// in a wrapping row, then each long field as a label over full-width text, with no gutter. That is
// the layout of `records-stack` (below), so the list gets that class. The rule is per list, not per
// card, so the cards of one list look alike. Steps are never changed by it.
//
// The layout is src/styles/records.css. The cells keep their inner HTML, so links, ids, inline code
// and the break points of code-breaks.mjs come along; the text of the page does not change. The
// short fields come first in the HTML, then the long ones, each group in the column order of the
// table, so the order on the screen is the order of the markup (Tab, screen readers, copy).
//
// Which tables:
//   - on a page under project/ (the pages mirrored from docs/, not project/index.html), a table
//     whose longest body cell has AUTO_MIN characters of text or more;
//   - any table, on any page, that has a comment on the line directly above it:
//       <!-- site: records -->                 records
//       <!-- site: table -->                   a plain table, even where the rule above would convert it
//       <!-- site: records key=N -->           records with column N (1-based) as the key; default 1
//       <!-- site: records style=steps -->     steps; the build fails unless every key starts with a number
//       <!-- site: records style=cards -->     cards with the key in a gutter, even where the keys are
//                                              numbers or a field is longer than TITLED_MIN
//       <!-- site: records style=titled -->    titled cards, even where every field is short and where
//                                              the keys are numbers
//     key=N and style=... can be combined, each once. Without style, the keys and the length of the
//     fields decide.
//   GitHub shows the comment as nothing, so docs/ still renders as a normal table there.
// A directive that is not one of these, an option on `site: table`, or a key outside the columns
// fails the build; so does `cargo xtask docs-check-src`, which reads the same comments in the sources.
//
// Empty cells are left out. A cell of SHORT_FIELD characters or fewer is a field-short, else a
// field-long. When the median key is longer than STACK_KEY characters, or the list is titled (see
// above), the key goes above the fields in the whole list (a gutter of 12.5rem cannot hold a
// sentence, and wastes the width beside a long body), for steps as well where the key is long:
// there the number and the note sit in one row. The style also stacks below 36rem. In a list that
// stacks because the median key is a sentence (not for steps, and not for titled cards whose keys
// are short), the list also gets `records-sentence`: the key is then set in the body font at weight
// 600 instead of monospace, which is for identifiers (a file name, a flag, a number). The integration runs
// after Starlight, whose build step makes the Pagefind index from the tables as written, and before
// code-breaks.mjs, which then sees the records. Node built-ins only.
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

// A table converts by itself when a body cell has this many characters of text or more. Measured
// on the 13 mirrored pages (86 tables): 240 gives records for 36 tables, 160 for 48 and 120 for 61,
// and below 240 the records were taller than the table for most of them.
export const AUTO_MIN = 240;

// A list whose median key is longer than this many characters stacks the key above the fields.
export const STACK_KEY = 48;

// A field whose cell has this many characters of text or fewer is a short fact (a small label over
// its value, side by side with the others); a longer one is a label over full-width text.
export const SHORT_FIELD = 40;

// A list in which a field has more than this many characters of text is a list of titled cards (the
// key above the fields, no gutter): a gutter of 12.5rem beside a 400-character paragraph is mostly
// empty space. A field of exactly this many characters does not trigger it.
export const TITLED_MIN = 400;

// A directive comment (the only comments that start with `site:`) followed by the table. The
// `(?!-->)` keeps a comment from reaching past its own `-->` to a later table.
const TABLE =
  /(<!--\s*site:(?:(?!-->)[\s\S])*-->\s*)?<table(?:\s[^>]*)?>([\s\S]*?)<\/table>/g;

const decode = (s) =>
  s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#(?:39|x27);/g, "'")
    .replace(/&amp;/g, '&');

// Characters of text in a piece of cell HTML: tags (and so <wbr> and the code-break spans) add none.
const textLength = (html) => [...decode(html.replace(/<[^>]+>/g, ''))].length;

const USAGE = 'use records, table, records key=N or records style=steps|cards|titled';

// { mode: 'records' | 'table', key, style: 'steps' | 'cards' | 'titled' | null } from the comment text; throws
// on anything else.
export function parseDirective(comment) {
  const m = /^<!--\s*site:\s*(.*?)\s*-->/s.exec(comment.trim());
  const words = m[1].split(/\s+/).filter(Boolean);
  const mode = words[0];
  if (mode !== 'records' && mode !== 'table') {
    throw new Error(`unknown directive \`site: ${m[1]}\` (${USAGE})`);
  }
  let key = null;
  let style = null;
  for (const w of words.slice(1)) {
    const k = /^key=(\d+)$/.exec(w);
    const s = /^style=(.*)$/.exec(w);
    if (mode === 'table') {
      throw new Error(`\`site: table\` takes no options, found \`${w}\` in \`site: ${m[1]}\``);
    }
    if (k && Number(k[1]) >= 1) {
      if (key !== null) throw new Error(`key given twice in \`site: ${m[1]}\``);
      key = Number(k[1]);
    } else if (s) {
      if (s[1] !== 'steps' && s[1] !== 'cards' && s[1] !== 'titled') {
        throw new Error(
          `unknown style \`${s[1]}\` in \`site: ${m[1]}\` (style=steps, style=cards or style=titled)`,
        );
      }
      if (style !== null) throw new Error(`style given twice in \`site: ${m[1]}\``);
      style = s[1];
    } else {
      throw new Error(`bad option \`${w}\` in \`site: ${m[1]}\` (${USAGE}; N >= 1)`);
    }
  }
  return { mode, key: key ?? 1, style };
}

function parseTable(inner) {
  const rows = [...inner.matchAll(/<tr>([\s\S]*?)<\/tr>/g)].map((r) =>
    [...r[1].matchAll(/<(th|td)(?:\s[^>]*)?>([\s\S]*?)<\/\1>/g)].map((c) => c[2].trim()),
  );
  return { header: rows[0] ?? [], body: rows.slice(1) };
}

const median = (nums) => {
  const s = [...nums].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
};

// `html` without one pair of parentheses that wraps all of its text, tags kept: "(done)" and
// "<strong>(done)</strong>" lose them, "(a) and (b)" and "(a) b" do not (the first "(" must be the
// first character of the text and be closed by its last one).
function withoutParens(html) {
  const at = []; // index in `html` of each character of text, outside tags
  let inTag = false;
  for (let i = 0; i < html.length; i++) {
    if (html[i] === '<') inTag = true;
    else if (html[i] === '>') inTag = false;
    else if (!inTag) at.push(i);
  }
  if (at.length < 2 || html[at[0]] !== '(' || html[at[at.length - 1]] !== ')') return html;
  let depth = 0;
  for (let n = 0; n < at.length; n++) {
    const c = html[at[n]];
    if (c === '(') depth++;
    else if (c === ')' && --depth === 0 && n < at.length - 1) return html;
  }
  const [open, close] = [at[0], at[at.length - 1]];
  return html.slice(0, open) + html.slice(open + 1, close) + html.slice(close + 1);
}

// { num, note } of a key cell that starts with a whole number followed by the end, white space or
// "(" ("6.22" and "A1" do not); `note` is the rest without one pair of surrounding parentheses, as
// HTML, or '' when there is none. Null for any other key.
export function stepKey(html) {
  const m = /^(\d+)(?=\s|\(|$)([\s\S]*)$/.exec(html.trim());
  if (!m) return null;
  const note = withoutParens(m[2].trim()).trim();
  return { num: m[1], note: textLength(note) === 0 ? '' : note };
}

export function recordsHtml({ header, body }, key, style = null) {
  const k = key - 1;
  const keys = body.map((r) => r[k] ?? '');
  const parsed = keys.map(stepKey);
  const allSteps = parsed.length > 0 && parsed.every(Boolean);
  if (style === 'steps' && !allSteps) {
    const bad = keys.findIndex((_, i) => !parsed[i]);
    throw new Error(
      `\`site: records style=steps\` needs every key (column ${key}) to start with a number, ` +
        `but row ${bad + 1} has \`${decode(keys[bad].replace(/<[^>]+>/g, '')).slice(0, 60)}\``,
    );
  }
  const steps = style === 'steps' || (style === null && allSteps);
  // The longest field of the whole list (a cell outside the key column, empty ones count as 0)
  const longest = Math.max(
    0,
    ...body.flatMap((row) => header.map((_, i) => (i === k ? 0 : textLength(row[i] ?? '')))),
  );
  // Titled: forced by style=titled, automatic when a field is over TITLED_MIN; never for steps, and
  // style=cards keeps the gutter. It is the stacked layout, so it shares the class.
  const titled = !steps && (style === 'titled' || (style === null && longest > TITLED_MIN));
  const sentenceKeys = median(keys.map(textLength)) > STACK_KEY;
  const stack = titled || sentenceKeys;
  // A key that is a sentence (the same median test as the stack) is set in the body font, not as a
  // monospace identifier. Only the median decides: titled cards with short keys stay identifiers,
  // and the number and note of a step are never a sentence key.
  const sentence = sentenceKeys && !steps;
  const tag = steps ? 'ol' : 'ul';
  const cls =
    `records${stack ? ' records-stack' : ''}${steps ? ' records-steps' : ''}` +
    (sentence ? ' records-sentence' : '');
  const out = [`<${tag} class="${cls}" role="list">`];
  body.forEach((row, r) => {
    const cells = header.map((_, i) => row[i] ?? ''); // a short row has empty cells
    // The short fields come first in the HTML, each group in the column order of the table: the
    // order a reader sees is then the order of the markup, of the Tab key, of a screen reader and
    // of a copy. (The stylesheet used to move them with `order`, which left focus in table order.)
    const items = header
      .map((h, i) => {
        if (i === k || textLength(cells[i]) === 0) return null;
        const size = textLength(cells[i]) <= SHORT_FIELD ? 'short' : 'long';
        return {
          size,
          html: `<div class="field field-${size}"><dt>${h || `Column ${i + 1}`}</dt><dd>${cells[i]}</dd></div>`,
        };
      })
      .filter(Boolean);
    const fields = [
      ...items.filter((f) => f.size === 'short'),
      ...items.filter((f) => f.size === 'long'),
    ]
      .map((f) => f.html)
      .join('');
    const { num, note } = steps ? parsed[r] : {};
    const keyHtml = steps
      ? `<span class="record-num">${num}</span>` +
        (note ? `<span class="record-note">${note}</span>` : '')
      : cells[k];
    out.push(
      `<li class="record"><div class="record-key">${keyHtml}</div>` +
        (fields ? `<dl class="record-fields">${fields}</dl>` : '') +
        '</li>',
    );
  });
  out.push(`</${tag}>`);
  return out.join('\n');
}

// Returns { html, converted, steps, tables } for one built page. `project` is true for a page that
// is mirrored from docs/, where the automatic rule applies. Throws on a bad directive or key.
export function convertTables(html, { project = false } = {}) {
  let converted = 0;
  let steps = 0;
  let tables = 0;
  const out = html.replace(TABLE, (all, comment, inner) => {
    tables++;
    const t = parseTable(inner);
    if (t.header.length === 0 || t.body.length === 0) return all;
    const directive = comment ? parseDirective(comment) : null;
    let key = 1;
    let style = null;
    let records;
    if (directive) {
      if (directive.mode === 'table') return all;
      key = directive.key;
      style = directive.style;
      records = true;
      if (t.header.length < 2 || key > t.header.length) {
        throw new Error(
          `\`site: records key=${key}\` on a table with ${t.header.length} column(s)`,
        );
      }
    } else {
      records =
        project &&
        t.header.length >= 2 &&
        Math.max(0, ...t.body.flat().map(textLength)) >= AUTO_MIN;
    }
    if (!records) return all;
    let list;
    try {
      list = recordsHtml(t, key, style);
    } catch (e) {
      const first = decode(t.header.join(' | ').replace(/<[^>]+>/g, '')).slice(0, 80);
      throw new Error(`table \`${first}\`: ${e.message}`);
    }
    converted++;
    if (list.startsWith('<ol')) steps++;
    return list;
  });
  return { html: out, converted, steps, tables };
}

async function* htmlFiles(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* htmlFiles(file);
    else if (entry.name.endsWith('.html')) yield file;
  }
}

export default function records() {
  return {
    name: 'uscreen-records',
    hooks: {
      'astro:build:done': async ({ dir, logger }) => {
        const root = fileURLToPath(dir);
        let pages = 0;
        let converted = 0;
        let steps = 0;
        let tables = 0;
        for await (const file of htmlFiles(root)) {
          const rel = path.relative(root, file).split(path.sep).join('/');
          const before = await readFile(file, 'utf8');
          let result;
          try {
            result = convertTables(before, {
              project: rel.startsWith('project/') && rel !== 'project/index.html',
            });
          } catch (e) {
            throw new Error(`${rel}: ${e.message}`);
          }
          tables += result.tables;
          if (result.converted > 0) {
            await writeFile(file, result.html);
            pages++;
            converted += result.converted;
            steps += result.steps;
          }
        }
        logger.info(
          `${converted} of ${tables} tables became records (${steps} of them steps), in ${pages} pages`,
        );
      },
    },
  };
}
