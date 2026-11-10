<!--
  docs/assets/README.md: what the images in this directory are, and what is still missing.
  Owns the list of images and the rule for adding one; where they are used is in README.md and website/.
-->
# Site and README images

## Here now

| Path | What it is |
| --- | --- |
| `screenshots/*.png` | the desktop app (Tauri), captured from its development mode, which shows **sample data** (a made-up tablet and serial, not a connected device). Dark theme is the main one; `light-*` are the same pages in the light theme. Captured at 1280×800 with a device scale of 2 |
| `screenshots/tour.gif` | an animated tour made from the dark screenshots (mode settings, a device page, system health, the add-device wizard, advanced) |
| `social-preview.svg`, `social-preview.png` | the repository's social-preview image |

Copies of the screenshots used by the documentation site are in `website/public/img/`; keep them in step.

## Still missing: real photos and a recording

A photo or a short recording of a real tablet next to a laptop with a window moved onto it would show the one thing the screenshots cannot.
When one exists, add it here and use it in `README.md` and on the site's landing page (`website/src/content/docs/index.mdx`):

| file | size | used by |
| --- | --- | --- |
| `uscreen-hero.jpg` | 1600×1000, under 300 KB | the top of `README.md` and the landing page |
| `uscreen-demo.mp4` or `.gif` | under 10 MB | a "See it" section |
| `uscreen-social.png` | 1280×640 | the repository's Social preview (uploaded by hand in **Settings → General → Social preview**; there is no API for it) |

Do not add mock-ups drawn by hand: a screenshot of the real app is fine if it says what it shows (sample data or a live device).

## Captures on the landing page

`website/public/real/` (screens) and `website/public/video/` (two recordings) are the real desktop app running its development mode on sample data. `website/scripts/capture/` has the scripts and the recipe.
