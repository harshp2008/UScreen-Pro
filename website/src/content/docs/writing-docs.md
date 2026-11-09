---
title: "Writing the docs"
---

How the pages of this site are written and checked.

## The truth rule

- Every statement comes from a file in the repository or from a command that was run. The page names the file in its `## Sources` list.
- Anything else is written as "not verified", or as "read from source, not run".
- Do not invent a symptom, a step, a number or a version. If the repository records no cause, write that it records none.
- No release status ("in beta", "from 1.x") on a timeless page. The changelog, the release notes and the Build field of an experiment record are history and keep their version words.
- The only platform run is Ubuntu 24.04 with GNOME 46 on Wayland. Anything about another system says "not verified".

## Anatomy of a page

A page has front matter with a `title`, a body that starts at `##` (Starlight prints the title), and, last, an optional `## Not verified` and a `## Sources` list. Links never end in `.md`.

## Where pages come from

| Pages | Source | Committed |
| :--- | :--- | :--- |
| the hand-written guide | `website/src/content/docs/*.md` | yes |
| Project documents | `docs/*.md`, `SECURITY.md`, `THIRD_PARTY_LICENSES.md`, mirrored by `website/scripts/generate.mjs` | no, generated |
| Release notes, Changelog | `CHANGELOG.md` | no, generated |
| CLI reference | the `--help` of the built `uscreen` | no, generated |
| Configuration | the config structs in `host/src/config/` | no, generated |
| Experiment records | `experiments_and_findings/*.md`, converted by `website/scripts/convert-experiments.mjs` | yes |
| API documentation | `cargo doc`, copied by `website/scripts/copy-rustdoc.mjs` | no |

## Personal data

Every generated and converted page passes `website/scripts/redact.mjs` (home paths, the maintainer's login name, e-mail addresses, IP and MAC addresses, device serials, tokens). The build ends with `check-site.mjs`, which fails if the built pages still match one of the patterns, if a link or an anchor does not resolve, or if a page asks for something from another site. Raw evidence (screenshots, logs) stays in `experiments_and_findings/` and is not published.

## Building the site

```bash
cd website
npm ci
cargo build -p uscreen          # the CLI reference needs the binary
RUSTDOCFLAGS="--document-private-items" cargo doc --no-deps -p uscreen   # from the repository root
npm run build
```

`SITE_BASE` sets the base path (default `/UScreen-Pro/`).

## Sources

- `website/scripts/`, `website/astro.config.mjs`, `.github/workflows/docs.yml`
- the reference conventions at <https://harshp2008.github.io/spawn-at/>
