# bingux-ui

The design system for Bingux's Freya applications: tokens that follow the
GNOME accent and colour scheme, the bundled icon set (the chat icon fork), the shell's
controls (switch, slider, segmented, dropdown, stepper, text field,
checkbox, radio, progress, spinner), buttons, the row and group family, the
navigation split view, status pages, banners, alert dialogs and toasts.

The full guide, component table and rules are in
[`docs/bingux-ui.md`](../../docs/bingux-ui.md).

```sh
# Every component, live
cargo run --example gallery

# Every gallery page rendered headlessly to /tmp/bingux-ui-gallery
cargo run --features testing --example snapshot
```

Icons under `icons/` come from the chat icon fork of Phosphor (MIT); see `icons/COPYING` and the Icons section of the guide.
