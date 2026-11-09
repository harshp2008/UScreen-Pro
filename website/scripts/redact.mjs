// scripts/redact.mjs: removes personal data from text before it is published on the site.
// Owns the redaction rules (home paths, user names in paths, e-mail addresses, IP addresses, MAC addresses,
// adb serials, tokens) and the counts of what was replaced. Does not own where text comes from (generate.mjs
// and convert-experiments.mjs call it) and never touches the repository files: only the output.
//
// The rules are deliberately blunt: a false positive costs a placeholder in a page, a miss publishes
// personal data. `npm run build` ends with check-site.mjs, which fails if any pattern below still matches
// in the built pages.

const counts = new Map();

/** Counts one replacement of [kind] in [where]. */
function note(kind, where) {
  const key = `${kind}`;
  counts.set(key, (counts.get(key) ?? 0) + 1);
  void where;
}

/** Documentation-range and local addresses that are not personal and stay as they are. */
const KEEP_IP = /^(127\.|0\.0\.0\.0$|255\.|22[4-9]\.|23\d\.|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.|192\.168\.1\.10[0-3]$|10\.238\.4\.100$|169\.254\.)/;

/** Patterns tested by check-site.mjs on the built output (the same rules, as regular expressions). */
export const LEAK_PATTERNS = {
  'home path': /\/home\/(?!USER\b)[a-z][\w.-]*\//,
  'macOS home path': /\/Users\/(?!USER\b)[A-Za-z][\w.-]*/,
  'e-mail address': /[\w.+-]+@[\w-]+\.[\w.-]+/,
  'user name': /\bharsh\b/,
  'IP address': /\b(?!127\.|0\.0\.0\.0|255\.|22[4-9]\.|23\d\.|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.|192\.168\.1\.10[0-3]\b|10\.238\.4\.100\b|169\.254\.)\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/,
  'MAC address': /\b(?!00:11:22:33:44:55)[0-9a-fA-F]{2}(?::[0-9a-fA-F]{2}){5}\b/,
};

/** Addresses that look like e-mail addresses but are not personal: the extension's uuid, git's remote user and the placeholder. */
export const ALLOWED_EMAILS = new Set(['uscreen-app-lock@uscreen.app', 'git@github.com', 'user@example.com']);

/** Replaces personal data in [text]; [where] is only used for the counts. */
export function redact(text, where = '') {
  let out = text;
  const sub = (kind, re, repl) => {
    out = out.replace(re, (...m) => {
      note(kind, where);
      return typeof repl === 'function' ? repl(...m) : repl;
    });
  };
  // Links into the maintainer's checkout become GitHub links; bare paths become repository-relative.
  sub('home path', /file:\/\/\/home\/[^/\s)]+\/Documents\/GITHUB(?:%20| )PROJECTS\/UScreen\//g, 'https://github.com/harshp2008/UScreen-Pro/blob/main/');
  sub('home path', /\/home\/[^/\s'")]+\/Documents\/GITHUB(?: |%20)PROJECTS\/UScreen\/?/g, '');
  sub('home path', /\/home\/(?!USER\b)[a-z][\w.-]*\//g, '~/');
  sub('home path', /\/Users\/(?!USER\b)[A-Za-z][\w.-]*/g, '/Users/USER');
  sub('home path', /C:\\Users\\[^\\\s]+/g, 'C:\\Users\\USER');
  // The extension's uuid (uscreen-app-lock@uscreen.app) and git's own remote name look like e-mail addresses; they are not.
  sub('e-mail address', /[\w.+-]+@[\w-]+\.[\w.-]+/g, (m) => {
    if (m === 'uscreen-app-lock@uscreen.app' || m === 'git@github.com') {
      counts.set('e-mail address', (counts.get('e-mail address') ?? 1) - 1);
      return m;
    }
    return 'user@example.com';
  });
  // The maintainer's login and host name (journal lines, process lists): a whole word, never the GitHub name harshp2008.
  sub('user name', /\bharsh\b/g, 'user');
  sub('MAC address', /\b(?!00:11:22:33:44:55)[0-9a-fA-F]{2}(?::[0-9a-fA-F]{2}){5}\b/g, '00:11:22:33:44:55');
  const seen = new Map();
  sub('IP address', /\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b/g, (ip) => {
    if (KEEP_IP.test(ip)) {
      counts.set('IP address', (counts.get('IP address') ?? 1) - 1);
      return ip;
    }
    if (!seen.has(ip)) seen.set(ip, `192.0.2.${seen.size + 10}`);
    return seen.get(ip);
  });
  sub('token', /\b(?:ghp|gho|ghs|ghu)_[A-Za-z0-9]{20,}\b|\bxox[bp]-[A-Za-z0-9-]{10,}\b|\bsk-[A-Za-z0-9]{20,}\b/g, 'TOKEN');
  // adb serials: 10-16 upper-case letters and digits with at least one of each (R52X708AEHK).
  sub('device serial', /\b(?!SHA\d)(?=[A-Z0-9]{10,16}\b)(?=(?:[A-Z0-9]*[A-Z]){3})(?=(?:[A-Z0-9]*\d){3})[A-Z0-9]{10,16}\b/g, 'DEVICE_SERIAL');
  return out;
}

/** One line per kind with the number of replacements made so far. */
export function redactionSummary() {
  return [...counts.entries()].map(([k, v]) => `${k}: ${v}`).join(', ') || 'nothing replaced';
}
