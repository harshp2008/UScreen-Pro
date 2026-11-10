<!--
  website/README.md: how to build the documentation site and where its pages come from.
  Owns the build instructions of the site; the rules for writing pages are in src/content/docs/writing-docs.md.
-->
# UScreen documentation site

A [Starlight](https://starlight.astro.build/) (Astro) site, built like the one of the spawn-at project: system fonts, no
analytics, no third-party request, search with Pagefind from files in the build output.

```bash
cd website
npm ci
cargo build -p uscreen                                                   # the CLI reference is the binary's --help
(cd .. && RUSTDOCFLAGS="--document-private-items" cargo doc --no-deps -p uscreen)   # the API documentation
npm run build                                                            # generate pages, build, copy rustdoc, check
npm run dev                                                              # local server with the generated pages
```

The site is served from `https://harshp2008.github.io/UScreen-Pro/`. `SITE_BASE` (default `/UScreen-Pro/`) and `SITE_URL`
change that. Pages come from three places (see `src/content/docs/writing-docs.md`): hand-written guide pages, pages generated
by `scripts/generate.mjs` from `docs/`, `CHANGELOG.md`, the CLI and the config source (not committed), and the experiment
records converted once by `scripts/convert-experiments.mjs` (committed).
