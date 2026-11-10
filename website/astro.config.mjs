// Config of the documentation site. Served from https://harshp2008.github.io/UScreen-Pro/ .
// The site makes no third-party request: system fonts, no analytics, no CDN, and the search is
// Pagefind, which runs from files in the build output.
import { readFileSync } from 'node:fs';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import mdx from '@astrojs/mdx';
import codeBreaks from './src/integrations/code-breaks.mjs';
import records from './src/integrations/records.mjs';

// Inline script for the <head> of every page: the button that hides the "On this page" column.
// It is inline so the saved choice is applied before the first paint.
const tocToggle = readFileSync(new URL('./src/scripts/toc-toggle.js', import.meta.url), 'utf8');

// The base path of a GitHub project site is the repository name. SITE_BASE overrides it for a
// custom domain or a fork (for example SITE_BASE=/ for a site at the root of its own domain).
const base = process.env.SITE_BASE ?? '/UScreen-Pro/';

export default defineConfig({
  site: process.env.SITE_URL ?? 'https://harshp2008.github.io',
  base,
  // Smart punctuation would turn `--id` in prose into an en dash and `...` into an ellipsis.
  // scripts/check-site.mjs fails the build if a flag-like dash pair is rewritten.
  markdown: { smartypants: false },
  integrations: [
    starlight({
      title: 'UScreen Pro',
      description: 'Use an Android tablet as a second display, a laptop touchpad or a pen tablet on Linux.',
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/harshp2008/UScreen-Pro' },
      ],
      customCss: [
        './src/styles/custom.css',
        './src/styles/tables.css',
        './src/styles/records.css',
        './src/styles/scroll-hints.css',
        './src/styles/toc-toggle.css',
        './src/styles/landing.css',
      ],
      head: [{ tag: 'script', content: tocToggle }],
      // The order follows the reader's path: the introduction, then installing and using it (with
      // the commands), where it works, the reference, the findings and experiments, the documents
      // of the project, the release notes, and the pages for people who work on it last. Every
      // group is `collapsed`: Starlight opens only the group that holds the current page. The
      // groups filled with `autogenerate` need no entry for a new page.
      sidebar: [
        { slug: 'index', label: 'Home' },
        { slug: 'what-is-uscreen-pro', label: 'What is UScreen Pro' },
        {
          label: 'Using UScreen',
          collapsed: true,
          items: [
            'install',
            'requirements',
            'update',
            'installer-flags',
            'files-and-paths',
            'quick-start',
            'concepts',
            {
              label: 'Commands',
              collapsed: true,
              items: [{ autogenerate: { directory: 'commands' } }],
            },
            'configuration',
            'environment',
            'troubleshooting',
          ],
        },
        {
          label: 'Where it works',
          collapsed: true,
          items: [
            {
              label: 'Environments',
              collapsed: true,
              items: [{ autogenerate: { directory: 'environments' } }],
            },
            'shortcomings',
          ],
        },
        {
          label: 'Reference',
          collapsed: true,
          items: [
            'known-limits',
            // The page is `rustdoc`, not `api`: the rustdoc output is copied to /api/, so a page
            // at /api/ would collide with it.
            { slug: 'rustdoc', label: 'API documentation (rustdoc)' },
            {
              label: 'CLI reference',
              collapsed: true,
              items: [{ autogenerate: { directory: 'reference' } }],
            },
            'settings-and-protocol',
          ],
        },
        {
          label: 'Findings and experiments',
          collapsed: true,
          items: ['findings', { autogenerate: { directory: 'experiments' } }],
        },
        // Written from docs/ at build time by scripts/generate.mjs; the order comes from the
        // `sidebar.order` of each page.
        {
          label: 'Project documents',
          collapsed: true,
          items: [{ autogenerate: { directory: 'project' } }],
        },
        // Written from CHANGELOG.md at build time by scripts/generate.mjs: the index first, then
        // the releases newest first.
        {
          label: 'Release notes',
          collapsed: true,
          items: [{ autogenerate: { directory: 'release-notes' } }],
        },
        {
          label: 'Developing',
          collapsed: true,
          items: ['testing', 'installer-tests', 'contributing', 'writing-docs', 'changelog'],
        },
      ],
    }),
    // MDX only for the landing page (src/content/docs/index.mdx), which uses Starlight's card components. After
    // Starlight, because Starlight's code-block integration must come before it.
    mdx(),
    // After Starlight on purpose: its build-done step makes the Pagefind index, and these then
    // rewrite the pages, so the search index is made from the text as written. Records come
    // before code-breaks, which then adds its break points inside the records as well.
    records(),
    codeBreaks(),
  ],
});
