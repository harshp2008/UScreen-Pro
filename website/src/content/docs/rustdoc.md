---
title: "API documentation (rustdoc)"
---

The Rust documentation of the daemon, generated from the `///` and `//!` comments with private items included (the daemon is a binary crate, so nearly everything is private).

[Open the API documentation](/UScreen-Pro/api/uscreen/index.html)

It is built by `RUSTDOCFLAGS="--document-private-items" cargo doc --no-deps -p uscreen` and copied into the site's `api/` folder by `website/scripts/copy-rustdoc.mjs`. Warnings are errors in CI. The page is called `rustdoc`, not `api`, because the generated output lives at `/api/`.

## Sources

- `website/scripts/copy-rustdoc.mjs`, `.github/workflows/docs.yml`
- `docs/comment-style.md`
