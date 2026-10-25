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
        let mut page = use_state(|| Page::General);
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
                page(),
                move |chosen: Page| page.set(chosen),
                content,
            )))
    }
}
```

The split view does not own the selected page. `selected` is the page the sidebar
shows, and `on_select` is called with the page a sidebar row chooses, so the caller
keeps that state. On a narrow window, `.list(shown, on_show_list)` decides whether the
list of pages or the page is the screen. `on_show_list` is called with `true` when the
list is asked for (Back from a page, Ctrl+F) and with `false` when a page is chosen.
Without `.list`, the split view keeps that state itself.

`OverlayHost` provides the handle returned by `use_overlays()`, which opens
dialogs and shows toasts from any component below it. It mounts `WindowHost`
inside itself, so popups need no extra wrapper at the root.

### Application loop and addresses

`Application` is the process loop, like `GtkApplication`. It reads the address
named on the command line, keeps one copy of the app running, and opens the
window:

```rust
fn main() {
    Application::new("my-app") // the app id, also the window's Wayland app id
        .scheme("my-app") // addresses look like my-app://page/subpage
        .title("My App")
        .borderless() // Gnoblin draws the frame
        .usage("Usage: my-app [ADDRESS]") // printed by --help
        .run(|| App);
}
```

- `size(w, h)` and `min_size(w, h)` set the window size. They default to
  `theme::WINDOW_SIZE` and `theme::WINDOW_MIN_SIZE`.
- A second launch does not open a second window. It sends its address to the
  running copy over `$XDG_RUNTIME_DIR/<app-id>.sock` and exits with status 0.
  The running copy opens the address and raises its window. A socket left by a
  crash is replaced.

An `Address` is a scheme, path segments and an optional query, for example
`my-app://network/wired/enp1s0?tab=ipv4`. Segments are percent-encoded, so a
segment can hold spaces or slashes.

| Call                         | Result                                                  |
| ---------------------------- | ------------------------------------------------------- |
| `Address::parse(text)`       | A full address. `None` for a bare path or a bad escape. |
| `Address::parse_in(s, text)` | A full address, or a bare path that takes scheme `s`.   |
| `join(seg)`, `parent()`      | One segment deeper, or one segment up.                  |
| `with_query(k, v)`, `query`  | Set or read a query value.                              |
| `to_string()`                | The address as text, `scheme://seg/seg?k=v`.            |

Components read the current address with `use_address()`, which redraws them
when it changes. Any code, including an event handler, moves to another address
with `navigate(address)`. `current_address()` reads it without subscribing.

## Components

| Family           | Components                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Controls         | `switch`, `checkbox` (tri-state), `slider` (`step`, `show_value`, `fill`), `segmented` (`fill` for equal segments; only the selected segment is raised, a hovered one brightens its text), `dropdown` (`app_icons`, `width`), `stepper`, `usage_meter` (a figure, its total and a thin bar), `action_strip` of `strip_action` (two to four icon-over-label actions in one rounded bar), `text_field` (`password`, `invalid`, `clearable`), `search_field`, `progress_bar`, `spinner`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Buttons          | `action_button` (`suggested`, `destructive`, `flat`, `loading`, `icon`), `primary_button`, `row_button`, `icon_button` (`tooltip`, `danger`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Rows             | `row` (`subtitle`, `value` (selectable, one line), `badge`, `leading`, `leading_space` (a blank icon column so text lines up), `leading_app`, `navigates`, `selected`, `destructive`, `context_menu`, `on_press` (makes the row activatable)), `radio_row(title, selected, on_select)` (a radio on the leading edge; the whole row selects; `subtitle`, `value`, `leading_app` as for `row`), `radio_rows(title, rows)` (a group of radio rows: Up and Down move the selection, and only the selected row is in the Tab order), `switch_row` (`nested` for sub-rows shown while on), `slider_row` (a `level_row` for a setting), `level_row` (title on the left and a slider filling the rest of the line; `control` such as a mute button sits between them, `footer` under the track, `mark` and `mark_label` for a tick such as 100%, `danger_from`, `value_text`, `max`, `leading`, `context_menu`), `choice_row`, `text_row`, `entry_row`, `expander_row`, `property_row` |
| Icons            | `icon` (bundled symbolic glyphs), `app_icon` (full-colour application icons from the icon theme)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Groups and text  | `Group` (`description`), `section_label`, `caption`, `centered_caption`, `action_row`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Layout           | `hstack(Space)`, `vstack(Space)`, `spacer`, `divider`, `card`, `clamp(Dim, child)`, `page_column`, `tile_rows(tokens, tiles, height)` (two tiles a row on a phone, four on a desktop)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Shell            | `NavigationSplitView` with `NavItem` and `subpage`, `HeaderBar`, `WindowControls`, `status_page`, `banner`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Page header      | `PageHead` (`subtitle`, `action`, `switch`): the title block that starts each page's content, with a status line under the title and one action or master switch at the right                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Search           | `NavigationSplitView::search(query, results)`: while the sidebar search has text, `results` replaces the page. Ctrl+F focuses it and Escape clears it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Search highlight | `search_highlight::provide(query)` in the results pane, then `search_highlight::label(style, tokens, text)` or `paragraph(...)` for text below it: each matched word gets an accent tint. Outside a search, or with no match, the text draws exactly as `TextStyle` would. While a search has text, a `Row` whose text does not match renders nothing, and a `Group` with no visible row hides with its title. `Group::unfiltered()` keeps every row for a group whose rows matched by other text, such as an index entry                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Overlays         | `dialog` with `ResponseKind::{Default, Suggested, Destructive}`, `Overlays::toast`, `Overlays::toast_with_action`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Popups           | `popup(title, content)` with `Popup::size`, `Popup::tabs` and `Popup::address`, opened by `window.open(popup)` from `use_window()`. A popup is a modal panel of its own, centred in the window: a Cancel button on the left, the title in the middle, the primary action on the right. It is 600 px tall unless `Popup::size` says otherwise, and it is clamped to the window. The body declares the primary action with `use_popup_actions(PopupActions { primary: Some(PopupAction { .. }) })`, and the caller closes the popup with `window.close()` once its work succeeds. Build the body with `popup_column()` and put each block in a `Group`, so the cards stand out from the panel. Cancel and Escape close it; Enter runs the primary action. The panel grows to at most 80% of the window height, and the body scrolls past that                                                                                                                                    |
| Forms            | `form_row(label, control)` for a label beside a control; `segmented(options, selected, on_select).fill()` for a popup tab rail of equal segments (it stays a rail on a phone)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Trees            | `tree_view(roots, expanded, selected)` takes `TreeNode`s (`id`, `label`, `detail`, `icon`, `badge`, `children`) and the sets of open branches and the selected id as state. Rows are `MenuItem` height. Up and Down move the selection, Right opens a branch or enters it, Left closes it or goes to the parent, Enter and Space activate. A click selects; clicking the selected row activates it. `TreeView::on_hover` and `TreeView::on_activate` report to the caller                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Splits           | `split_pane(first, second, ratio)` with the first pane's share of the length in `ratio` (kept between 0.15 and 0.85). Side by side by default; `.vertical()` stacks the panes. Drag the divider to resize; the divider is a hairline with an `S1` hit zone on each side                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Menus            | `menu_item` and `menu_button(label, items)`. `MenuItem::icon`, `MenuItem::checked`, `MenuItem::destructive` and `MenuItem::disabled`; `menu_separator()`. `Overlays::show_menu` opens at window coordinates and stays on screen: it opens upward or leftward when there is no room. Picking an item closes the menu, then runs it; Escape and an outside press close it. While a menu or dropdown list is open, the window behind it takes no presses, wheel turns or hover, and the wheel never scrolls the page behind a list                                                                                                                                                                                                                                                                                                                                                                                                                                                |

Every control supports keyboard use: Enter and Space activate. Arrows move
sliders and steppers. Page Up and Page Down move sliders and steppers by ten
steps, and Home and End go to the ends. Open dropdown menus take arrows, Page
Up, Page Down, Home and End, and a typed letter jumps to the next option that
starts with it. Enter, Space and the arrows open a closed dropdown. Escape closes
dropdowns, menus, popups and dialogs. Segmented controls and radio groups are
roving: only the selected item is in the tab order, and the arrows move the
selection and focus together (Up and Down also work in a vertical radio group,
wrapping at the ends). Text fields: Enter submits, and Escape reverts an edited
value and then leaves the field. A password field has a reveal button, and a right
click opens Cut, Copy, Paste and Select all. In a popup, the panel takes focus on
open unless a field is marked `autofocus()`, and Enter in a single-line field runs
the primary action. Each control has an accessibility role and a focus ring.
Animations are short (80 to 200 ms) and stop when `BINGUX_REDUCED_MOTION=1` is set.

Menus take the keyboard too. Enter, Space or Down on a menu button opens its
menu with the first item highlighted. Up and Down move the highlight over
enabled items and skip separators; Home and End go to the first and last item,
and a typed letter jumps to the next item that starts with it. An item made
with `submenu(label, items)` opens its submenu on hover after 150 ms, or on
Right, Enter or click. A pointer moving from an open row toward its submenu
keeps the submenu open while it stays inside the safe triangle between the
pointer and the submenu. Left and Escape step back one level; Escape at the top
level and Tab close the menu and return focus to the control that opened it.

Popups share one motion, all with an ease-out curve and all scaled by
`tokens.motion(ms)`, so they are instant under `BINGUX_REDUCED_MOTION=1`.
Each layer stays on screen until its exit finishes.

- Popup: fades in and scales from 94% to 100% in 150 ms. The scrim fades with it. Closing reverses it.
- Dialog: fades in and scales from 96% to 100% in 120 ms. The scrim fades with it.
- Menu: fades in and scales from 96% to 100% in 90 ms, pivoting on its top-left corner (the anchor).
- Toast: slides up 8 px and fades in over 140 ms. Dismissing it slides it back down and fades it out.
- Dropdown: fades and scales in over 90 ms, the same easing as the menu.

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
  `shadow(Elevation::Raised)`, `ink(Paint::Text)` (the
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

### Size class and density

Sizes come in two classes, `SizeClass`. `Regular` is the mouse density of a
desktop window; `Compact` is the fingertip density. The window root
(`WindowHost`) sets the class at the start of every frame, before any child
renders:

- `Compact` when the window is narrower than `theme::NARROW_BELOW` (740 px),
  a phone.
- `Compact` when the latest pointer event was a touch. A finger on a wide
  window flips the class to `Compact`; the next mouse press or move flips it
  back to `Regular`. Pointer kind is latched from the root's global pointer
  listeners, so this is the only way the window switches density by itself.

Every size token resolves against the current class when it is applied. Each
kind has one table in `style/tokens.rs`, so tuning a size is one edit. The
values are:

| Token                               | Regular         | Compact         |
| ----------------------------------- | --------------- | --------------- |
| `Dim::Row`                          | 56 px           | 60 px           |
| `Dim::RowSubtitle`                  | 63 px           | 68 px           |
| `Dim::Control`                      | 36 px           | 44 px           |
| `Dim::Button`                       | 40 px           | 48 px           |
| `Dim::MenuItem`                     | 36 px           | 52 px           |
| `Dim::NavItem`                      | 40 px           | 56 px           |
| `Dim::Header`                       | 48 px           | 56 px           |
| `Dim::SwitchWidth` x `SwitchHeight` | 36 x 20 px      | 48 x 28 px      |
| `Dim::SliderThumb`                  | 16 px           | 22 px           |
| `Dim::StepperHalf`                  | 28 px           | 36 px           |
| `Dim::Field`                        | 240 px          | 168 px          |
| `Space::S3` / `S4` / `S5`           | 12 / 16 / 20 px | 14 / 18 / 22 px |
| `Space::S6` / `S8` / `S12`          | 24 / 32 / 48 px | 26 / 36 / 54 px |
| `TextSize::Sm` / `Md` / `Lg`        | 13 / 14 / 16 px | 14 / 15 / 17 px |
| `TextSize::Heading` / `Display`     | 18 / 28 px      | 19 / 30 px      |
| `IconSize::Control` / `Row`         | 16 / 18 px      | 18 / 20 px      |
| `IconSize::App`                     | 32 px           | 36 px           |

Sizes that are not in the table are the same at both classes. The chart
geometry, menu edge and display arrangement use `Space::fixed_px()`, which
reads the regular value whatever the class, because they are constants.

The API, all in `bingux_ui::style` and the prelude:

- `SizeClass::{Regular, Compact}`, with `is_compact()` and `pick(regular,
compact)`.
- `size_class()` and `is_compact()` read the class now. Reading either while a
  component renders subscribes that component, so it re-renders when the class
  changes. `pick(regular, compact)` is the same read for a value that is not a
  token.
- `PointerKind::{Mouse, Touch}` and `pointer_kind()` give the latched pointer
  kind.
- `Space::px()`, `TextSize::px()`, `IconSize::px()` and `Dim::px()` resolve at
  the current class. A style applied with `tw` or `font()` needs no call.

The class is one `State` created once per thread, so it survives every
re-render. The root writes it before any child renders, so a frame that
changes the class is consistent: no child sees a mix of densities.

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
- **Compact density is the fingertip size class.** A window narrower than
  `NARROW_BELOW` (740 px), or one a finger has touched, resolves every size
  token to its compact value (see
  [Size class and density](#size-class-and-density)). A dropdown in a row is
  capped at `Dim::Field` (168 px) so the row's title keeps its room. A
  segmented control becomes a dropdown, since a track of segments does not fit
  beside a title. A row title wraps onto a second line instead of being cut. A
  change of class re-renders the components that read a size, in place, with
  no remount.
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
  fit it. `subtitle_one_line()` keeps the subtitle to one line, so the row has
  its natural height, `Dim::RowSubtitle` (63 px at mouse density). A list that
  mounts rows lazily sizes its slots with that token.
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
- **Blocking work runs on the executor.** `task::spawn_blocking(f)` runs `f`
  on a bounded worker pool (four to eight threads, following the CPU count) and
  returns a future a Freya `spawn` task can `.await`. `task::use_resource(key,
load)` loads a `Resource<T>` in the background: it is `Loading`, then
  `Ready(T)`, or `Error(String)` when `load` fails; `refresh()` reloads it and
  keeps the old value as `Refreshing(T)`. The load stops when its component
  unmounts. The `runtime` helpers (`background`, `background_then`,
  `use_cached`, `prefetch`, `optimistic`) use the same pool.
- **Persistent cache.** A page's last good value survives a restart.
  `runtime::load_persistent(key, load)` runs `load` and saves its `Ok` value as
  JSON at `$XDG_CACHE_HOME/<app>/<key>.json` (or under `~/.cache`). The app
  name is the program's file name. `prefetch_persistent(key, load)` at start-up
  runs that load in the background. `cached_persistent::<T, E>(key)` in a hook
  gives the state, seeded from the saved file when the state is first created.
  That is one small local read, the only file read allowed in a hook, so the
  page has its data at its first render. A missing, unreadable or
  other-format file counts as no value. Use it for a page's slow model, not for
  live state.
- **Never on the UI thread.** Do not run `Command`, `runtime::run`, a file
  read, `apps::installed()`, a gsettings or `bingux-config` call in `render`,
  a `use_hook` or `use_memo` body, or directly in an event handler. Put it in
  `use_resource`, `background` or `optimistic`, and render the previous value
  or `Loading` until it returns.
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
  reach a child's `on_press`. Scrollbars in the dropdown list and the popup
  auto-hide, so they are not pinned open.
- **An action row** (`action_row`) is a form's last row: a small muted
  note at the left and a compact button at the right, on the row height and
  the gutter. Put it inside a group, or under the last group on a page.
- **A message** (loading, empty, error) is a `status` row inside a group; only
  an error is tinted. A page that has no data yet shows `skeleton()`, or `skeleton_shape(&[rows per section])` when its loaded shape is known, so the final sections are in place before the data arrives.
- **Chrome.** The header bar carries the window buttons, Back and the busy
  spinner. On a desktop window the sidebar and the page are flush, split by a
  hairline, and the page header sits on the content colour. A page starts with
  its title block: `PageHead::new()` as the first child of `page_column`, which
  draws the page's name (`PAGE_TITLE`), a `PageHead::subtitle` status under it
  and a `PageHead::action` at the right. A phone draws the same title block,
  under a header that carries only Back.
  The header title is a pure function of the scroll offset (`ScrolledTitle`): it
  fades in and rises between 24 and 56 px of scroll, so it reverses exactly as
  the page scrolls back. A hero page (one whose hero already names it, such as
  About, an application's identity, a wired or VPN connection, a Bluetooth
  device or an account) starts with its hero and adds no `PageHead`.
  The sidebar is a search field, the signed-in person
  (`NavigationSplitView::profile`, which opens `Profile::target`), then rows
  under `NavItem::group` names. Every sidebar row, page, profile or search
  result, is the public `SidebarRow` component (`SidebarMark::Icon` or
  `SidebarMark::Avatar`), so they share one icon column and label inset, a
  neutral selection layer and an accent icon on the selected row.
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
  subpage slides in from the right over 250 ms, ease-out, over the page it was
  opened from. That page stays in place and dims slightly, and the new page has
  a soft shadow on its leading edge. A pop slides the top page out to the right,
  revealing the page below, which undims. A second navigation during a slide
  starts from where the page is, with no snap. A sidebar switch cross-fades over
  120 ms. Motion is off under reduced motion (`tokens.motion(ms)` returns 0).
- Icons are Adwaita symbolic SVGs under `packages/bingux-ui/icons/` with
  the hard-coded fill removed so they recolour. Add one by copying it from
  `/usr/share/icons/Adwaita/symbolic/` and adding a variant to `Icon`.
- A dropdown draws a symbolic icon before each option with `.icons(Vec<Icon>)`,
  in the same order as the options. Sound uses it for each device's kind.

## Motion

`motion.rs` holds the animated building blocks. Use them instead of hand-rolled
animation so motion never shifts the layout around it. Each keeps layout stable:

- `reveal(open, child)` animates its own height, so the content below moves
  smoothly instead of jumping.
- `presence(open, child)` fades and scales in place. It keeps its space until
  the exit ends, then releases it. Chain `.collapse()` to animate that space
  away too, as a `reveal` does.
- `crossfade(child)` swaps the child with a fade and holds the larger of the old
  and new sizes during the swap, so the container never shrinks mid-swap. When
  the new child is smaller, the box settles to it once the swap ends. Wrap the
  child in `reveal` if that step must animate.
- `use_tween(target, ms)` and `animated_number(value)` change values, not the
  size of the box they sit in. Give numbers a fixed width if neighbours must not
  move.

Durations go through `tokens.motion(MOVE_MS)`, so reduced motion makes every
motion an instant change. `use_presence` is the hook under `presence`; overlays,
menus and dropdowns use it directly.

### Keyboard focus

Controls draw their focus ring by ending `render` with
`.focusable(focus(), radius)`, from the `Focusable` trait in `motion.rs`. It
wraps the control in a row that takes the control's size, and the ring is a
child of that row. Give the wrapper any grow or fill style the control had
(for example `w_full()` or `w_flex()`). For a row inside a clipped container
edge, such as a card's, use `.focusable_inside(focus(), radius)`: the ring
becomes a child of the control, so it sizes to the control's own box and the
clip cannot cut it off. The ring is a 2 px accent border set outside the
control. It grows out from the control and fades in when keyboard focus
arrives (`FOCUS_IN_MS`), then fades out when focus leaves (`FOCUS_OUT_MS`).
Pointer focus draws no ring. The ring takes the control's corner radius plus
its outset, so it follows the shape of the control.

Window focus lives in `focus.rs`. `WindowHost` provides one `FocusCore` for the
window; Tab and F6 move between focus regions (the sidebar, the content, the
header). A region draws its ring around the whole area, not around one control.
Clicking a control claims the press, so a pointer press never starts a ring.
`trap()` wraps a popup so Tab stays inside it while it is open. Use
`use_focus_core()` to reach the core: with no host (the gallery, the snapshot
runner, an inspector window) it returns a detached core, so focus moves do
nothing and controls still work.

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
