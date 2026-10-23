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

| Family          | Components                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Controls        | `switch`, `checkbox` (tri-state), `radio_group`, `slider` (`step`, `show_value`, `fill`), `segmented`, `dropdown` (`app_icons`, `width`), `stepper`, `usage_meter` (a figure, its total and a thin bar), `action_strip` of `strip_action` (two to four icon-over-label actions in one rounded bar), `text_field` (`password`, `invalid`, `clearable`), `search_field`, `progress_bar`, `spinner` |
| Buttons         | `action_button` (`suggested`, `destructive`, `flat`, `loading`, `icon`), `primary_button`, `row_button`, `icon_button` (`tooltip`, `danger`)                                                                                                                                                                                                                                                     |
| Rows            | `row` (`subtitle`, `value`, `badge`, `leading`, `leading_app`, `navigates`, `selected`, `destructive`), `switch_row`, `slider_row` (a `level_row` for a setting), `level_row` (title and value over a full-width slider, with optional leading and trailing elements), `choice_row`, `text_row`, `entry_row`, `expander_row`, `property_row`                                                     |
| Icons           | `icon` (bundled symbolic glyphs), `app_icon` (full-colour application icons from the icon theme)                                                                                                                                                                                                                                                                                                 |
| Groups and text | `Group` (`description`), `section_label`, `caption`, `centered_caption`, `action_row`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                                                                                                                                                                  |
| Layout          | `hstack`, `vstack`, `spacer`, `divider`, `card`, `clamp`, `page_column`                                                                                                                                                                                                                                                                                                                          |
| Shell           | `NavigationSplitView` with `NavItem` and `subpage`, `HeaderBar`, `WindowControls`, `status_page`, `banner`                                                                                                                                                                                                                                                                                       |
| Page header     | `PageHeader` (`subtitle`, `action`) with `use_page_header`, a title, a muted subtitle under it and one action at the end                                                                                                                                                                                                                                                                         |
| Search          | `NavigationSplitView::search_hits` with `SearchHit`: pages first, then matching settings, Up, Down and Enter, at most 20 results. Ctrl+F focuses it and Escape clears it                                                                                                                                                                                                                         |
| Overlays        | `dialog` with `ResponseKind::{Default, Suggested, Destructive}`, `Overlays::toast`, `Overlays::toast_with_action`                                                                                                                                                                                                                                                                                |
| Sheets          | `sheet` with `Sheet::primary` (`SheetAction`), `Sheet::tabs` (a tab strip under the title), `Overlays::show_sheet`, `Overlays::close_sheet`. Cancel and Escape close it; Enter runs the primary action. Sheets size to their content and scroll past 80% of the window height                                                                                                                    |
| Forms           | `form_row(label, control)` for a label beside a control; `tabs(options, selected, on_select)` for an underlined tab strip with a roving tab stop: Left and Right move selection and focus, Home and End go to the ends                                                                                                                                                                           |
| Menus           | `menu_item` and `menu_button(label, items)`. `MenuItem::icon`, `MenuItem::checked`, `MenuItem::destructive` and `MenuItem::disabled`; `menu_separator()`. `Overlays::show_menu` opens at window coordinates and stays on screen: it opens upward or leftward when there is no room. Picking an item closes the menu, then runs it; Escape and an outside press close it                          |

Every control supports keyboard use: Enter and Space activate. Arrows move
sliders and steppers. Page Up and Page Down move sliders and steppers by ten
steps, and Home and End go to the ends. Open dropdown menus take arrows, Page
Up, Page Down, Home and End, and a typed letter jumps to the next option that
starts with it. Enter, Space and the arrows open a closed dropdown. Escape closes
dropdowns, menus, sheets and dialogs. Segmented controls and radio groups are
roving: only the selected item is in the tab order, and the arrows move the
selection and focus together (Up and Down also work in a vertical radio group,
wrapping at the ends). Text fields: Enter submits, and Escape reverts an edited
value and then leaves the field. A password field has a reveal button, and a right
click opens Cut, Copy, Paste and Select all. In a sheet, the panel takes focus on
open unless a field is marked `autofocus()`, and Enter in a single-line field runs
the primary action. Each control has an accessibility role and a focus ring.
Animations are short (80 to 200 ms) and stop when `BINGUX_REDUCED_MOTION=1` is set.

Popups share one motion, all with an ease-out curve and all scaled by
`tokens.motion(ms)`, so they are instant under `BINGUX_REDUCED_MOTION=1`.
Each layer stays on screen until its exit finishes.

- Sheet: fades in and drops 12 px in 140 ms. The scrim fades with it. Closing reverses it.
- Dialog: fades in and scales from 96% to 100% in 120 ms. The scrim fades with it.
- Menu: fades in and scales from 96% to 100% in 90 ms, pivoting on its top-left corner (the anchor).
- Toast: slides up 8 px and fades in over 140 ms. Dismissing it slides it back down and fades it out.
- Dropdown reveal: 90 ms, the same easing as the menu.

## Charts

`charts` draws sparklines, line and area charts, bar charts, rings and
donuts. Each chart fills the width it is given and keeps a fixed height from
its first frame, so data that arrives later never moves the layout. Before it
has points it shows a muted baseline and "No data yet" at the same height.
A single x value is centred with one tick at the value, not a -1 to 1 axis. A constant series sits in the middle of a window and labels its one value. Time axes span the data, not whole hours.

```rust
use bingux_ui::prelude::*;

line_chart([Series::new("Charge", points)])
    .y_unit(Unit::Percent)
    .curve(Curve::Monotone)
    .area()
    .label("Battery charge over the last day")
```

| Builder              | Use                                                                            |
| -------------------- | ------------------------------------------------------------------------------ |
| `sparkline(values)`  | A small trend with no axes. `colour`, `unit`, `area`, `label`.                 |
| `line_chart(series)` | Several series over x. `y_unit`, `x_unit`, `curve`, `area`, `label`, `height`. |
| `bar_chart(series)`  | Grouped bars, or `stacked()`. `categories`, `y_unit`, `label`, `height`.       |
| `ring(value)`        | A 0 to 1 gauge with a centre caption. `caption`, `colour`, `label`.            |
| `donut(segments)`    | A share of a whole, with a legend. `unit`, `caption`, `label`.                 |

`Series::new(name, points)` takes `(x, y)` pairs, and `Series::values` reads
values by position. `Segment::new(name, value)` is one slice of a donut.

`Unit` formats values: `Number`, `Percent` (values already in percent),
`Bytes` (binary units), `Duration` (seconds as `2 h 15 min`) and
`Clock { offset }` (Unix seconds in local time). `Curve` is `Linear`, `Step`
or `Monotone` (the default, a smooth curve that never overshoots).

Data handling: a `NaN` or infinite point breaks the line instead of joining
across it. Points are sorted by x, and a repeated x keeps its last value.

Motion: lines draw in from left to right, bars grow from the baseline and
rings sweep, over 700 ms. Under `BINGUX_REDUCED_MOTION=1` they appear at once.

Keyboard: a chart with points is focusable. Left and Right move between
points, Home and End jump to the ends, and a tooltip follows the point.

## Typography

`typography.rs` holds the type ramp. Text picks a named style and nothing
sets its own size or weight, so each level reads apart at a glance: size
steps of at least 1.25x, and a weight or colour change at each step. Use
`styled_label` and `styled_paragraph`, or `TextStyle::label` and
`TextStyle::paragraph`, to render a style.

| Style      | Size | Weight   | Colour  | Use                                     |
| ---------- | ---- | -------- | ------- | --------------------------------------- |
| `HERO`     | 36   | light    | primary | One figure a page is about.             |
| `TITLE`    | 28   | semibold | primary | A page or subpage name.                 |
| `HEADING`  | 18   | semibold | primary | A group title above its card.           |
| `BODY`     | 14   | normal   | primary | Row titles and body text.               |
| `EMPHASIS` | 14   | semibold | primary | A name or state inside body text.       |
| `VALUE`    | 14   | normal   | muted   | Read-only value at the right of a row.  |
| `NUMERIC`  | 14   | normal   | primary | Changing numbers, with tabular figures. |
| `CAPTION`  | 12   | normal   | muted   | Row subtitles, descriptions and hints.  |
| `BADGE`    | 11   | semibold | accent  | Short tags such as "New".               |
| `OVERLINE` | 11   | semibold | muted   | Tracked small capitals above a block.   |
| `LINK`     | 14   | medium   | accent  | Inline links and actions.               |

`Ink::Danger` recolours any style for destructive text. Weights come from
`theme::with_font`. In the gallery render semibold reads clearly heavier than
normal in Adwaita Sans; medium is only checked by eye, so re-check it if
`LINK` ever looks like normal text.

## Rules

`bingux-ui` is the shell's look in Freya. The source of truth is the shell's
own QML, not a reference app: `shell/bingux/Theme.qml`, `ControlRow.qml`,
`ControlSwitch.qml`, `ControlCentreButtonSurface.qml`, `ActionButton.qml` and
`SeekSlider.qml`. When the shell changes, change these to match.

- **A near-black, flat look.** The page is `bg`, the sidebar a step darker,
  popups (sheets, dialogs, menus) a step lighter with a hairline outline and
  a soft shadow. Buttons, fields, dropdowns and segmented controls use
  `control`. Hover, pressed and selected are white alpha layers over any
  base, and selection is tinted with the accent. Text is `#ececf0`,
  secondary text `#8d8f9a`.
- **One scale, in `src/scale.rs`.** Spacing is `S0` to `S7` on a 4 px grid.
  Radius is 10 for row and control layers, 14 for rows' hover layer and
  cards, 20 and more for popups, and `RADIUS_PILL` for buttons, fields,
  dropdowns, steppers and segmented controls. A row is 48 px, or 60 px with
  a subtitle, and keeps that height. A control in a row is 32 px; a
  standalone button and a menu item are 36 px. Dropdowns and fields in a
  row share one 220 px column (`FIELD_WIDTH`); a field for a URL or a path
  uses `FIELD_WIDTH_WIDE` (440 px).
- **`scripts/ui-lint.py` enforces it.** It fails on a raw number in a spacing,
  padding, margin, radius, font-size, width, height or icon-size call, and on
  `FontWeight::BOLD`. To use a new value, add it to `scale.rs` first.
  `make check` runs it.
- **One inset.** `GUTTER` (12) is the only horizontal inset: row text,
  section titles and sidebar items start `GUTTER` from their container.
- **A group is flat.** A small muted medium-weight name sits above rows
  that lie directly on the page: no card, no outline, no hairlines. Space,
  size and weight make the hierarchy; a row's hover and selection layer is a
  rounded fill behind it. A page starts with its title in the serif display
  face (the first installed of Newsreader, Source Serif, Noto Serif,
  Liberation Serif, DejaVu Serif; the interface font when none is), a muted
  subtitle line and the page's master switch at the right, then its groups.
- **Controls are pills.** The switch is 48 x 28 with a light track and a dark
  thumb when on. The selected segment of a segmented control is a light pill
  with dark text. Buttons, fields, dropdowns and steppers are fully rounded.
  The slider has a thin track and a round thumb. A disabled row
  (`row(..).enabled(false)`) dims and disables every control in it.
- **Popups are modals in the display face.** A sheet or alert dialog has a
  26 px display-face title at the top left, content, and Cancel and the
  primary action as pills at the bottom right; sheets add a tab strip under
  the title.
- **An action row** (`action_row`) is a form's last row: a small muted
  note at the left and a compact button at the right, on the row height and
  the gutter. Put it inside a group, or under the last group on a page.
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
- **Application icons** (`app_icon`, `Row::leading_app`) are looked up as
  the dock looks them up: the current GTK icon theme, the themes it
  inherits, `hicolor`, then `/usr/share/pixmaps`; an absolute path is used as
  it is. Sizes are `ICON_APP_SM` (20, in a dropdown), `ICON_APP` (32, in a
  row) and `ICON_APP_LG` (64, at the head of a page). An icon that is not
  found draws a tile with the generic application glyph.
- **A subpage** is `NavigationSplitView::subpage(title, on_back)`. The
  header shows a Back button and the title, and Alt+Left and the mouse Back
  button call `on_back`. The application keeps the stack; the sidebar
  selection decides which page owns it.
- **Busy pages** call `use_page_busy(bool)` (from the prelude). While it is
  true the header shows a small spinner beside the title, with a "Working"
  tooltip. Never show a loading block in the page: it pushes the content down.
- **Page motion** runs in the split view's content column, not per page. A
  subpage slides in from the right over 160 ms and the page behind it fades;
  a pop slides out to the right. A sidebar switch cross-fades over 120 ms.
  Motion is off under reduced motion (`tokens.motion(ms)` returns 0).
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
