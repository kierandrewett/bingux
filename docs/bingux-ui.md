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

- Read colours from `use_tokens()`. The only literal colours are white and
  black overlays. Surfaces are opaque: page `bg`, `sidebar`, `card`, and the
  white overlays `elevated`, `hover`, `pressed` and `selection` for states.
- One size scale: rows are 50 px, or 62 px with a subtitle; controls and
  buttons are 34 px; group and card radius is 12 px, row and button radius
  8 px; row text is 14 px and secondary text 12 px. Rows are 16 px from the
  group edge, and group titles line up with row text.
- Spacing belongs to the parent. A page is `page_column()`, which puts 24 px
  between groups. Groups, rows and status rows carry no outer margin.
- The header bar shows the page title. A page has no title of its own.
- Sidebar icons are monochrome. The selected item has a neutral fill. Colour
  is for the accent in controls: switches, sliders, checkmarks, suggested
  buttons.
- Buttons, dropdowns, segmented controls, steppers and text fields share one
  fill (`elevated`) with no idle outline. Hover and pressed raise it.
- A message in a page (loading, empty, error) is a `status` row of the same
  height as every other row, not loose text. Use a `banner` for an error
  that needs an action.
- The window is opaque and square. Gnoblin draws the corners, border and
  shadow, so an application sets no corner radius, transparency or shadow on
  its root.
- Load Adwaita Sans with `theme::with_font(LaunchConfig::new())`. The font is
  variable; Freya only picks real weights for fonts it loads itself, and
  otherwise fakes bold, which looks blotchy.
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
