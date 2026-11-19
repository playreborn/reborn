# 0009: Iced as GUI framework

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

Reborn needs a native, lightweight GUI without an embedded browser
(plan sections 1 and 10). The GUI is thin: it shows daemon state and sends
commands (ADR 0002, 0008).

## Alternatives considered

1. **Iced** (MIT): pure Rust, Elm-like architecture, wgpu with tiny-skia
   fallback, async tasks, image support, built-in QR code widget. 0.14
   (December 2025) needs Rust 1.88, our MSRV.
2. **egui** (MIT/Apache): immediate mode; quick for tools, harder to give a
   polished, consistent look.
3. **Slint**: GPL-3.0 or commercial/royalty-free licenses with conditions;
   its own markup language.
4. **GTK / Qt bindings**: native look, but heavy C/C++ dependencies and
   weaker Windows story.

## Decision

Iced 0.14 for `apps/reborn`. All logic stays in `rebornd`/`reborn-core`;
the GUI only renders state and sends IPC requests.

## Consequences

- Known gaps to watch: screen-reader support, IME for chat (M3), rich text
  for store descriptions (M6).
- Because the GUI is thin, switching frameworks later stays feasible.
