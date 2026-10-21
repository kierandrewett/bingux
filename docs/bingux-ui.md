# bingux-ui

`packages/bingux-ui` is the design system for Bingux's native applications.
It is a Rust library on [Freya](https://freyaui.dev) with the shell's visual
system: the same surfaces, radii, rows, switches and sliders as the Control
Centre. Bingux Settings is built on it.

## Use it in an application

Add the crate and build the window root with `provide_tokens`, which reads
the GNOME accent and colour scheme, follows changes made elsewhere, and
styles Freya's own widgets to match:

```toml
[dependencies]
freya = { version = "0.5.0-rc.10", features = ["borderless", "titlebar"] }
bingux-ui = { path = "../bingux-ui" }
```

```rust
use bingux_ui::prelude::*;
use freya::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Page { General, About }

impl NavItem for Page {
    fn title(self) -> &'static str { match self { Page::General => "General", Page::About => "About" } }
    fn icon(self) -> Icon { match self { Page::General => Icon::PreferencesSystem, Page::About => Icon::About } }
}

#[derive(PartialEq)]
struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        let page = use_state(|| Page::General);
        let content = Group::new("Options".to_string())
            .row(switch_row("Enabled", true, |_| {}))
            .row(row("Version").value("1.0"));
        rect()
            .expanded()
            .color(tokens.text)
            .font_family(bingux_ui::theme::FONT_FAMILY)
            .child(OverlayHost::new(NavigationSplitView::new(
                "My App",
                vec![Page::General, Page::About],
                page,
                content,
            )))
    }
}
```

`OverlayHost` provides the handle returned by `use_overlays()`, which opens
dialogs and shows toasts from any component below it.

## Components

| Family          | Components                                                                                                                                                                                                         |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Controls        | `switch`, `checkbox` (tri-state), `radio_group`, `slider` (`step`, `show_value`), `segmented`, `dropdown`, `stepper`, `text_field` (`password`, `invalid`, `clearable`), `search_field`, `progress_bar`, `spinner` |
| Buttons         | `action_button` (`suggested`, `destructive`, `flat`, `loading`, `icon`), `primary_button`, `row_button`, `icon_button` (`tooltip`, `danger`)                                                                       |
| Rows            | `row` (`subtitle`, `value`, `badge`, `leading`, `navigates`, `selected`, `destructive`), `switch_row`, `slider_row`, `choice_row`, `text_row`, `entry_row`, `expander_row`, `property_row`                         |
| Groups and text | `Group` (`description`), `section_label`, `caption`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                      |
| Layout          | `hstack`, `vstack`, `spacer`, `divider`, `card`, `clamp`, `page_column`                                                                                                                                            |
| Shell           | `NavigationSplitView` with `NavItem`, `HeaderBar`, `WindowControls`, `status_page`, `banner`                                                                                                                       |
| Overlays        | `dialog` with `ResponseKind::{Default, Suggested, Destructive}`, `Overlays::toast`, `Overlays::toast_with_action`                                                                                                  |

Every control supports keyboard use: Enter and Space activate, arrows move
sliders, steppers and segmented controls, Up and Down walk the sidebar,
Escape closes dropdowns and dialogs. Each has an accessibility role and a focus ring. Animations are
short (80 to 200 ms) and stop when `BINGUX_REDUCED_MOTION=1` is set.

## Rules

`bingux-ui` is the shell's look in Freya. The source of truth is the shell's
own QML, not a reference app: `shell/bingux/Theme.qml`, `ControlRow.qml`,
`ControlSwitch.qml`, `ControlCentreButtonSurface.qml`, `ActionButton.qml` and
`SeekSlider.qml`. When the shell changes, change these to match.

- **Colours are the shell's `flatSurfaces` palette** (`Tokens::new`): the
  window is `popupSurface`, the sidebar `shellSurface`, group cards, dialogs
  and popovers `elevated`. Buttons, fields, dropdowns and segmented controls
  use `control`, one step lighter than `elevated`, so they stay visible on a
  card. Hover, pressed and selected are alpha layers drawn over
  any base, as `ControlCentreButtonSurface` does, and selection is tinted with
  the accent. Text is `#fafafa`, secondary text `#b0b0b0`.
- **One scale, in `src/scale.rs`.** Spacing is `S0` to `S7` on a 4 px grid.
  Radius is 8 for controls and row hover, 12 for group cards and 20 for
  dialogs, popovers and toasts (`RADIUS`, `RADIUS_LG`, `RADIUS_XL`). Type is
  14 body, 12 secondary, 16 section titles. A row is 40 px, or 56 px with a
  subtitle, and every row keeps that height. A control in a row (dropdown,
  field, segmented control, stepper, `compact()` or row button) is 32 px, so
  it leaves 4 px above and below. A standalone `ActionButton` and a menu item
  are 36 px. Dropdowns and fields in a row share one 220 px column
  (`FIELD_WIDTH`), so their left edges line up; a dropdown grows to fit a
  longer option.
- **`scripts/ui-lint.py` enforces it.** It fails on a raw number in a spacing,
  padding, margin, radius, font-size, width, height or icon-size call, and on
  `FontWeight::BOLD`. To use a new value, add it to `scale.rs` first.
  `make check` runs it.
- **One inset.** `GUTTER` (12) is the only horizontal inset: row text,
  section titles and sidebar items start `GUTTER` from their container.
- **A group is a card.** A small title sits above a 12 px card whose rows are
  flush against it, with a hairline between rows. A row is an optional icon,
  a normal-weight title, an optional small muted subtitle and trailing
  content. A page starts with its first group; there is no page title.
- **Controls are the shell's.** The switch is 36 x 20 with a 16 px thumb. The
  slider has a 6 px track (8 px while dragging), a text-coloured fill that
  turns accent while dragging and a 16 px thumb. Buttons are filled with
  `control` and have no idle outline; no control has one. A disabled row
  (`row(..).enabled(false)`) dims and disables every control in it.
- **A message** (loading, empty, error) is a `status` row inside a group; only
  an error is tinted. A page that has no data yet shows `skeleton()`, not text.
- **Chrome.** The header bar carries the window buttons only. The sidebar is
  a search field, then rows under `NavItem::group` labels, with the selection
  layer and an accent icon on the selected row.
- The window is opaque and square. Gnoblin draws the corners, border and
  shadow, so an application sets no corner radius, transparency or shadow on
  its root.
- Load the font with `theme::with_font(LaunchConfig::new())`. The family is
  `appearance.font_family` from `bingux-config`, else Adwaita Sans, else the
  system sans-serif; GNOME's font setting is not read. Many system fonts are
  variable. Freya only picks real weights for fonts it loads itself, and
  otherwise fakes bold, which looks blotchy, so `with_font` loads the files.
- Icons are Adwaita symbolic SVGs under `packages/bingux-ui/icons/` with
  the hard-coded fill removed so they recolour. Add one by copying it from
  `/usr/share/icons/Adwaita/symbolic/` and adding a variant to `Icon`.

## Gallery and snapshots

The gallery shows every component live:

```sh
cargo run --manifest-path packages/bingux-ui/Cargo.toml --example gallery
```

The `testing` feature adds `bingux_ui::snapshot`, which renders a window
without a compositor through `freya-testing` and writes a PNG. The snapshot
example renders every gallery page, then scripts a few interactions (the
dropdown open, a dialog, a toast):

```sh
cargo run --manifest-path packages/bingux-ui/Cargo.toml --features testing \
    --example snapshot -- /tmp/bingux-ui-gallery
```

Use `Snapshot::new(app, size)` with `settle`, `click`, `hover` and `save`
to script your own captures. Bingux Settings exposes the same renderer as
`bingux-settings --snapshot DIR` (see [settings](settings.md)).
