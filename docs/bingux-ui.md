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
            .tw(&tokens, &[w_full(), h_full()])
            .font_family(bingux_ui::theme::font_family())
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

| Family           | Components                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Controls         | `switch`, `checkbox` (tri-state), `radio_group`, `slider` (`step`, `show_value`, `fill`), `segmented`, `dropdown` (`app_icons`, `width`), `stepper`, `usage_meter` (a figure, its total and a thin bar), `action_strip` of `strip_action` (two to four icon-over-label actions in one rounded bar), `text_field` (`password`, `invalid`, `clearable`), `search_field`, `progress_bar`, `spinner`                                                                                                                                         |
| Buttons          | `action_button` (`suggested`, `destructive`, `flat`, `loading`, `icon`), `primary_button`, `row_button`, `icon_button` (`tooltip`, `danger`)                                                                                                                                                                                                                                                                                                                                                                                             |
| Rows             | `row` (`subtitle`, `value` (selectable, one line), `badge`, `leading`, `leading_space` (a blank icon column so text lines up), `leading_app`, `navigates`, `selected`, `destructive`, `context_menu`, `on_press` (makes the row activatable)), `switch_row` (`nested` for sub-rows shown while on), `slider_row` (a `level_row` for a setting), `level_row` (title and value over a full-width slider, with optional leading and trailing elements), `choice_row`, `text_row`, `entry_row`, `expander_row`, `property_row`               |
| Icons            | `icon` (bundled symbolic glyphs), `app_icon` (full-colour application icons from the icon theme)                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Groups and text  | `Group` (`description`), `section_label`, `caption`, `centered_caption`, `action_row`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                                                                                                                                                                                                                                                                                                          |
| Layout           | `hstack(Space)`, `vstack(Space)`, `spacer`, `divider`, `card`, `clamp(Dim, child)`, `page_column`                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Shell            | `NavigationSplitView` with `NavItem` and `subpage`, `HeaderBar`, `WindowControls`, `status_page`, `banner`                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Page header      | `PageHeader` (`subtitle`, `action`) with `use_page_header`, a status line that replaces the sentence under the page title, and one action at the right of the title                                                                                                                                                                                                                                                                                                                                                                      |
| Search           | `NavigationSplitView::search(query, results)`: while the sidebar search has text, `results` replaces the page. Ctrl+F focuses it and Escape clears it                                                                                                                                                                                                                                                                                                                                                                                    |
| Search highlight | `search_highlight::provide(query)` in the results pane, then `search_highlight::label(style, tokens, text)` or `paragraph(...)` for text below it: each matched word gets an accent tint. Outside a search, or with no match, the text draws exactly as `TextStyle` would. While a search has text, a `Row` whose text does not match renders nothing, and a `Group` with no visible row hides with its title. `Group::unfiltered()` keeps every row for a group whose rows matched by other text, such as an index entry                |
| Overlays         | `dialog` with `ResponseKind::{Default, Suggested, Destructive}`, `Overlays::toast`, `Overlays::toast_with_action`                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Sheets           | `sheet` with `Sheet::primary` (`SheetAction`), `Sheet::tabs` (a tab rail under the title), `Overlays::show_sheet`, `Overlays::close_sheet`. A sheet is a 520 px window of its own in the page colour: a flat Cancel on the left, the title in the middle, the primary action on the right. Build the body with `sheet_column()` and put each block in a `Group`, so the cards stand out from the sheet. Cancel and Escape close it; Enter runs the primary action. Sheets size to their content and scroll past 80% of the window height |
| Forms            | `form_row(label, control)` for a label beside a control; `tabs(options, selected, on_select)` for a rail of equal tabs with the selected one raised, as the segmented control, with a roving tab stop: Left and Right move selection and focus, Home and End go to the ends                                                                                                                                                                                                                                                              |
| Menus            | `menu_item` and `menu_button(label, items)`. `MenuItem::icon`, `MenuItem::checked`, `MenuItem::destructive` and `MenuItem::disabled`; `menu_separator()`. `Overlays::show_menu` opens at window coordinates and stays on screen: it opens upward or leftward when there is no room. Picking an item closes the menu, then runs it; Escape and an outside press close it. While a menu or dropdown list is open, the window behind it takes no presses, wheel turns or hover, and the wheel never scrolls the page behind a list          |

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

| Style        | Size | Weight   | Colour  | Use                                     |
| ------------ | ---- | -------- | ------- | --------------------------------------- |
| `HERO`       | 36   | light    | primary | One figure a page is about.             |
| `TITLE`      | 28   | semibold | primary | A name in an identity block.            |
| `HEADING`    | 18   | semibold | primary | A large value inside a chart or ring.   |
| `PAGE_TITLE` | 28   | medium   | primary | The name at the top of a page.          |
| `SECTION`    | 16   | semibold | primary | A group title above its card.           |
| `BODY`       | 14   | normal   | primary | Row titles and body text.               |
| `EMPHASIS`   | 14   | semibold | primary | A name or state inside body text.       |
| `VALUE`      | 14   | normal   | muted   | Read-only value at the right of a row.  |
| `NUMERIC`    | 14   | normal   | primary | Changing numbers, with tabular figures. |
| `CAPTION`    | 13   | normal   | muted   | Row subtitles, descriptions and hints.  |
| `BADGE`      | 11   | semibold | accent  | Short tags such as "New".               |
| `OVERLINE`   | 11   | semibold | muted   | Tracked small capitals above a block.   |
| `LINK`       | 14   | medium   | accent  | Inline links and actions.               |

`Ink::Danger` recolours any style for destructive text. Weights come from
`theme::with_font`. In the gallery render semibold reads clearly heavier than
normal in Adwaita Sans; medium is only checked by eye, so re-check it if
`LINK` ever looks like normal text.

Row values are drawn with `SelectableText` and `TextStyle::span`, not a
paragraph, so a user can select them with the mouse and copy them with Ctrl+C
or a right-click Copy. This covers the `value` of `row` and the value of
`property_row`. A value stays on one line, and a long value is cut with an
ellipsis.

`TextStyle::selectable` is the one path for selectable text that wraps. It
backs the `centered_caption`, the status block, the text field's invalid
message, and the subtitle of a row or level row that does not press. A
selectable text takes the pointer-down that a press starts from, so a row with
`on_press` keeps its subtitle plain: its press keeps working, and the subtitle
cannot be selected there. While a search marks words in a text, it is a
paragraph with the marks instead, so it is not selectable until the search
clears.

## Styling with tokens

Every box is styled by data, as Tailwind classes style HTML. The style
system is in `src/style/`:

- `tokens.rs` holds the values. Each kind is its own type, so a number does
  not compile where a token is expected:
    - `Space`: `S0`, `S0_5`, `S1` to `S8`, `S10`, `S12`, with Tailwind
      numbering (`S4` is 16 px, as `p-4` is). `GUTTER` is `S4`.
    - `Radius`: `Mark`, `Control`, `Card`, `Overlay`, `Tile` and `Pill`.
    - `Dim`: sizes by role, such as `Row`, `Control`, `Button` and `Field`.
      Square boxes are named by Tailwind size step: `Box5` (20 px), `Box7`,
      `Box9`, `Box11` and `Box24` (96 px).
    - `IconSize`, `Paint` (colour roles), `Line` (borders) and `Elevation`
      (shadows).
- `mod.rs` holds the utilities. Each is one fact about a box, for example
  `px(S4)`, `gap(S2)`, `rounded(Radius::Card)`, `bg(Paint::Card)`,
  `border(Line::Hairline)`, `h(Dim::Row)`, `flex_row()`, `grow_children()`,
  `items(Align::Center)`, `surface(base, hover, press, selected)`,
  `focus_ring(focus)`, `shadow(Elevation::Raised)`, `ink(Paint::Text)` (the
  text colour children inherit) and `font(BODY)` (for a component that draws
  its own text).
- `recipes.rs` holds the recipes: named lists of utilities such as `ROW`,
  `CARD`, `GROUP`, `CONTROL`, `MENU_ITEM` and `NAV_ITEM`. It also holds the
  spacing rhythm table, which sets the space between the page title,
  groups, cards and rows.

Apply utilities and recipes with `Tw::tw`. Later utilities override earlier
ones, and padding and margin merge side by side:

```rust
use bingux_ui::prelude::*;

let tokens = use_tokens();
rect()
    .tw(&tokens, ROW)
    .tw(&tokens, &[surface(Paint::Transparent, hovering, pressing, selected)])
    .child(BODY.label(&tokens, "Wi-Fi"))
```

Text uses a `TextStyle` from `typography.rs` with an `Ink`. An icon takes
an `IconSize` and a token colour: `Paint::Muted.color(&tokens)`.

`scripts/ui-lint.py` (run by `make check`) fails when code outside the style
system does any of these:

- Calls Freya's styling methods (`padding`, `spacing`, `margin`,
  `corner_radius`, `background`, `border`, `shadow`, `font_size`,
  `font_weight`, `direction`, `content`, alignment and `overflow`).
- Reads a colour field of `Tokens`.
- Uses a `Color` or `FontWeight` value.
- Gives a box a fixed size that is not a `Dim`.

A value computed at run time can still be passed to `Size::px`, such as a
measured width, a drag offset or a chart's geometry. To add a value, add a
token or a recipe first.

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
  any base, as `ControlCentreButtonSurface` does; selection is a neutral
  light layer. Text is `#fafafa`, secondary text `#b0b0b0`.
- **The token system is the only way to style a box.** See
  [Styling with tokens](#styling-with-tokens). A row is 56 px, or as tall as
  its subtitle needs. A control in a row (dropdown, field, segmented
  control, stepper, compact button) is 36 px. The page column is 720 px
  wide. A standalone button is 40 px and a menu item 36 px. Dropdowns and
  fields in a row share one 240 px column (`Dim::Field`), so their left
  edges line up; a field for a URL or a path uses `Dim::FieldWide` (440 px).
- **Phone width uses fingertip sizes.** Below 740 px the split view calls
  `set_touch(true)`: a row is 60 px, a sidebar page 56 px, a control 44 px, a
  standalone button 48 px, and the switch and slider handles grow. A
  dropdown, field or slider in a row is cut to 168 px so the row's text
  keeps room, and a dropdown there shows only its value and a chevron, with
  no box. A segmented control becomes the same value picker, since a track
  of segments does not fit beside a title. A row title wraps onto a
  second line instead of being cut. `Dim` resolves these sizes when a style is applied, so a style is
  written once. Crossing the threshold remounts the page once.
- **One inset.** `GUTTER` (16) is the only horizontal inset inside a card:
  row text, free content and the hairline start `GUTTER` from the card's
  edge.
- **A group is a card.** A 16 px semibold title sits above a 16 px card with a
  hairline edge, aligned with the card. Rows sit flush in it with a hairline
  between them, inset by the gutter on both sides. Every boxed surface (a group,
  the action strip, a free-content box) draws its fill, hairline and radius from
  the one `BOX` recipe, and `boxed(tokens)` builds one; do not draw a box by hand.
  A row is an optional icon, a normal-weight title, an optional muted subtitle
  that wraps to three lines and trailing content. A row with a subtitle grows to
  fit it.
- **Row hover.** Only an activatable row shows a hover fill and a pointer: one
  that navigates (a chevron), has `on_press`, or opens something. A switch,
  slider, dropdown, segmented, value-only or entry row shows neither. A switch
  row still toggles when its label is pressed, but it has no hover fill. The
  hairline above and below a row goes while an activatable row is hovered or
  pressed, or while it is selected, so its highlight reads as one block (`Row`
  reports this to its group).
- **Focus ring.** A row with `on_press` is focusable (a switch row included), and a keyboard-focused one draws the focus ring, as every control does.
- **A row's menu and sub-rows.** `Row::context_menu(items)` opens a menu of
  `menu_item`s at the pointer on a right click, as the menu button does. A
  `switch_row(title, on, handler).nested(on, rows)` reveals its sub-rows under
  the row while it is on, animated; the sub-rows sit inside
  the group's card, indented by the gutter.
- **Optimistic changes.** A control whose change is applied by a command
  (gsettings, a service, a file) calls `runtime::optimistic(shown, overlays,
previous, value, work)`. It shows `value` at once, runs `work` in the
  background and, if `work` returns `Err`, reverts `shown` to `previous` and
  toasts the error. `runtime::optimistic_then` also takes `done`, which runs on
  the UI thread after success, for a reload of a cached model. The control never
  waits on the command.
- **Controls are the shell's.** The switch is 36 x 20 with a 16 px thumb. The
  slider has a 6 px track (8 px while dragging), an accent fill and a 16 px
  white knob on a soft shadow. Buttons are filled with
  `control` and have no idle outline; no control has one. A disabled row
  (`row(..).enabled(false)`) dims and disables every control in it.
- **Segmented controls** sit on a `Control` rail with `Radius::Inner` segments.
  The selected segment is a `Card` chip with a `Knob` shadow. A usage meter's
  fill is `Accent`, not text.
- **Search fields** centre the search glyph vertically. The clear button
  acts on left pointer-down: a press on the Freya input's own handler does not
  reach a child's `on_press`. Scrollbars in the dropdown list and the sheet
  auto-hide, so they are not pinned open.
- **An action row** (`action_row`) is a form's last row: a small muted
  note at the left and a compact button at the right, on the row height and
  the gutter. Put it inside a group, or under the last group on a page.
- **A message** (loading, empty, error) is a `status` row inside a group; only
  an error is tinted. A page that has no data yet shows `skeleton()`, not text.
- **Chrome.** The header bar carries the window buttons, Back and the busy
  spinner. On a desktop window the page is a rounded panel inset in the
  window frame, which is the sidebar colour. A page starts with its title
  row: `PAGE_TITLE`, one sentence from `NavItem::lead` (or the page's
  `PageHeader::subtitle` status when it sets one) and the page's action at the
  right. On a phone the title is in the header and the sentence is the first
  line of the page. Once the title row scrolls out of view, the page title
  slides up and fades into the header bar, centred on the bar and inset past
  the wider of its two control clusters
  (`ScrolledTitle`, 220 ms, ease-out).
  The sidebar is a search field, the signed-in person
  (`NavigationSplitView::profile`, which opens `Profile::target`), then rows
  under `NavItem::group` names, with a neutral selection layer and an accent
  icon on the selected row.
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
  it is. Sizes are `IconSize::AppSmall` (20, in a dropdown), `IconSize::App`
  (32, in a row) and `IconSize::AppLarge` (64, at the head of a page). An icon that is not
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
- A dropdown draws a symbolic icon before each option with `.icons(Vec<Icon>)`,
  in the same order as the options. Sound uses it for each device's kind.

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
