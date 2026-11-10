---
title: "Environments"
sidebar:
  order: 0
  label: "Overview"
---

What works on which desktop. The first column is the only one that has been run.

| Environment | Status here | Page |
| :--- | :--- | :--- |
| GNOME on Wayland (Ubuntu 24.04, GNOME 46) | **the only tested platform** | [GNOME on Wayland](/UScreen-Pro/environments/gnome-wayland/) |
| GNOME on X11 | code exists; **not verified** | [GNOME on X11](/UScreen-Pro/environments/gnome-x11/) |
| KDE Plasma | code exists; **not verified** here | [KDE](/UScreen-Pro/environments/kde/) |
| Sway and other wlroots compositors | no integration; **not verified** | [Sway](/UScreen-Pro/environments/sway/) |
| Anything else | falls back to `generic` | [Unsupported desktops](/UScreen-Pro/environments/unsupported-desktops/) |

A desktop is supported by a *backend* that answers one question: what can it do? The answer is a set of capabilities (focus, pointer context, window placement, pen menu). A feature that needs a capability does nothing when the backend lacks it, so an unsupported desktop degrades instead of failing. Adding one is described in the guide under Project documents.

## Sources

- `host/src/desktop/mod.rs` (`Capabilities`, `detect`)
- `docs/desktop-backends.md`, `docs/compatibility.md`
