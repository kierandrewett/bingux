# bingux-ui

`packages/bingux-ui` is the design system for Bingux's native applications.
It is a Rust library on [Freya](https://freyaui.dev) with one visual system,
the one GNOME Settings uses: opaque layered greys, boxed groups of 50 px rows,
monochrome sidebar icons, one accent colour and one control size. Accent
colours follow the shell's `Theme.qml`. Bingux Settings is built on it.

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

These keep every page the same. A component that breaks one is a bug.

- **One scale, in `src/scale.rs`.** Spacing is `S0` to `S7` on a 4 px grid;
  radius is `RADIUS_SM`, `RADIUS`, `RADIUS_LG`; type is `TEXT_XS` to
  `TEXT_DISPLAY` (11, 12, 14, 16, 20, 28); icons are `ICON_SM`, `ICON_MD`,
  `ICON_LG`, `ICON_XL`; square boxes are `BOX_SM` to `BOX_2XL`. A control is
  `CONTROL_HEIGHT` high; a row is `ROW_HEIGHT`, or `TALL_ROW_HEIGHT` with a
  subtitle.
- **`scripts/ui-lint.py` enforces it.** It fails on a raw number in a spacing,
  padding, margin, radius, font-size, width, height or icon-size call, and on
  `FontWeight::BOLD`. Only 0.0 and 1.0 (a hairline) and halves or doubles of a
  name pass. To use a new value, add it to `scale.rs` first. `make check`
  runs it.
- **One inset.** `GUTTER` is the only horizontal inset. Row text, section
  titles and sidebar items all start `GUTTER` from their
  container, so they line up.
- Read colours from `use_tokens()`. The only literal colours are white and
  black overlays. Surfaces are opaque: `bg`, `sidebar`, and the white
  overlays `elevated`, `hover`, `pressed` and `selection` for states.
- **The page is flat and starts with its first section.** There is no page
  title, description or headline: the sidebar already names the page. A
  section is a bold title, an optional muted line, then rows with no card
  behind them and a hairline between rows. A page is `page_column()`, which
  puts `S6` between sections. A message (loading, empty, error) is a `status`
  row of normal height inside a section; only an error is tinted.
- **Chrome.** The header bar carries the window buttons only. The sidebar is
  a search field, then items under `NavItem::group` labels, with a neutral
  pill on the selected item.
- Buttons, dropdowns, segmented controls, steppers and text fields share one
  fill (`elevated`) and one 1 px outline (`field_border`). Hover and pressed
  raise the fill. The keyboard ring is `RING_WIDTH` in the accent colour.
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
