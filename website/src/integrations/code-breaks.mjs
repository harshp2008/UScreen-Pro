// Line breaks inside inline code. A browser breaks after a hyphen by default (`--wait-` /
// `strategy`), does not break after a `/`, and breaks anywhere between two characters once
// `overflow-wrap: anywhere` is set. This integration rewrites the built HTML so that inline code
// (a <code> outside <pre>) breaks only
//
//   - after `/`, after `::` and after `,`, where it adds a <wbr>, and
//   - at spaces, as before.
//
// A piece between two of those points is kept whole by the CSS (src/styles/custom.css). A piece
// with a character other than a letter, digit, `_` or `.` (a `-`, a `?`, a bracket after a symbol:
// places where the browser would break) is wrapped in <span class="code-nb"> (no break at all).
// A piece longer than LONG_PIECE characters is wrapped in
// <span class="code-any">, which may be cut anywhere: no table column should have to be as wide
// as a 50-character test name, so a piece that long is the one case where a cut inside a name is
// accepted. <wbr> and the spans add no characters: the text of every page, and what a reader
// copies, is the same as before (zero-width spaces would be copied into pasted paths and
// commands).
//
// It runs as an Astro integration after Starlight, whose own `astro:build:done` step builds the
// Pagefind index first, so the search index is made from the unchanged text. Node built-ins only.
// A rehype plugin would be the usual way, but `markdown.rehypePlugins` needs the package
// `@astrojs/markdown-remark`, which this site does not install.
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

// A piece longer than this many characters may be cut anywhere.
// 20 characters is about 155px of code in a table cell. Measured on the built site: at 20 the
// first table of the design-platforms page still fits its 720px column, at 24 it scrolls
// sideways; a lower value fits more tables and cuts more names, a higher one does the reverse.
// Every path and name that was cut in the middle in the bug report is shorter than this.
const LONG_PIECE = 20;

// A tag or a comment. Attribute values may contain `>` inside quotes.
const TOKEN =
  /<!--[\s\S]*?-->|<\/?[a-zA-Z][^\s>/]*(?:\s+[^\s"'>/=]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s"'>]+))?)*\s*\/?>/g;

// Splits one word (no whitespace) at the allowed points and returns it with <wbr> between the
// parts and the parts that need it wrapped.
function breakWord(word, longPiece) {
  const parts = [];
  let start = 0;
  for (let i = 0; i < word.length - 1; i++) {
    const c = word[i];
    const next = word[i + 1];
    const point =
      (c === '/' && next !== '/') ||
      (c === ',' && !/\s/.test(next)) ||
      (c === ':' && word[i - 1] === ':' && next !== ':');
    if (point) {
      parts.push(word.slice(start, i + 1));
      start = i + 1;
    }
  }
  parts.push(word.slice(start));
  return parts
    .map((part) => {
      // an entity such as &lt; is one character on the page
      const long = part.replace(/&[#a-zA-Z0-9]+;/g, '.').length > longPiece;
      if (long) return `<span class="code-any">${part}</span>`;
      // Letters, digits, `_` and `.` do not give the browser a break point; any other character
      // may (after `-` and `?`, before `(`, `[`, `<` and `{` that follow a symbol).
      return /[^A-Za-z0-9_.]/.test(part) ? `<span class="code-nb">${part}</span>` : part;
    })
    .join('<wbr>');
}

// The text between two tags inside an inline <code>.
function breakText(text, longPiece) {
  return text
    .split(/(\s+)/)
    .map((piece, i) => (i % 2 === 1 || piece === '' ? piece : breakWord(piece, longPiece)))
    .join('');
}

// Returns the page with the break points added to every inline <code>.
export function addCodeBreaks(html, longPiece = LONG_PIECE) {
  let out = '';
  let last = 0;
  let pre = 0;
  let code = false;
  TOKEN.lastIndex = 0;
  let m;
  while ((m = TOKEN.exec(html)) !== null) {
    const text = html.slice(last, m.index);
    out += code ? breakText(text, longPiece) : text;
    const tag = m[0];
    last = m.index + tag.length;
    const name = /^<\/?([a-zA-Z][^\s>/]*)/.exec(tag)?.[1].toLowerCase();
    const closing = tag.startsWith('</');
    if ((name === 'script' || name === 'style') && !closing && !tag.endsWith('/>')) {
      // raw text: copy it up to the closing tag untouched
      const end = html.toLowerCase().indexOf(`</${name}`, last);
      const stop = end === -1 ? html.length : end;
      out += tag + html.slice(last, stop);
      last = stop;
      TOKEN.lastIndex = stop;
      continue;
    }
    if (name === 'pre') pre += closing ? -1 : 1;
    else if (name === 'code') code = !closing && pre === 0;
    out += tag;
  }
  const rest = html.slice(last);
  return out + (code ? breakText(rest, longPiece) : rest);
}

async function* htmlFiles(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const file = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* htmlFiles(file);
    else if (entry.name.endsWith('.html')) yield file;
  }
}

export default function codeBreaks() {
  return {
    name: 'uscreen-code-breaks',
    hooks: {
      'astro:build:done': async ({ dir, logger }) => {
        let files = 0;
        let changed = 0;
        for await (const file of htmlFiles(fileURLToPath(dir))) {
          files++;
          const before = await readFile(file, 'utf8');
          const after = addCodeBreaks(before);
          if (after !== before) {
            await writeFile(file, after);
            changed++;
          }
        }
        logger.info(`break points added to inline code in ${changed} of ${files} pages`);
      },
    },
  };
}
