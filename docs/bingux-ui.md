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
| Controls         | `switch`, `checkbox` (tri-state), `slider` (`step`, `show_value`, `fill`, `gradient(stops)`: the track is a colour ramp instead of the accent fill; the night light's colour temperature uses it, and a hue or brightness ramp is the same call), `segmented` (`fill` for equal segments; only the selected segment is raised, a hovered one brightens its text), `dropdown` (`app_icons`, `width`), `stepper`, `usage_meter` (a figure, its total and a thin bar), `qr_code` (`ecc`, `label`, `size`: a pure-Rust QR encoder, versions 1 to 15, drawn black on a white tile with the quiet zone in every theme; `QrMatrix::encode` gives the modules; too-long data draws an empty tile), `action_strip` of `strip_action` (two to four icon-over-label actions in one rounded bar), `text_field` (`password`, `invalid`, `clearable`), `search_field`, `progress_bar`, `spinner` |
| Navigation       | `segmented(options, selected, on_select).tabs()` (the tab bar: each tab a real button with the segmented look, only the selected one raised, Left, Right, Home and End move the selection and focus; `scroll()` for a narrow container; an app's view tabs and the inspector's tabs use it too), `breadcrumb(crumbs, on_select)` (crumbs separated by chevrons, each a Tab stop, the last in normal ink; `code` for a code-face trail; a file manager path uses it too) |
| Buttons          | `action_button` (`suggested`, `destructive`, `flat`, `loading`, `icon`), `primary_button`, `row_button`, `icon_button` (`tooltip`, `danger`, `busy` (a spinner in place of the icon, the size unchanged), `radius`: `Control` like every button by default), `split_button(icon)` or `SplitButton::new(icon)` (a primary action and a menu in one control: `on_press`, `tooltip` or `label` (the primary segment's name), `menu(items)` or `on_menu(handler)` for the chevron, `menu_label` (the chevron's name), `enabled`; see Split buttons) |
| Rows             | `row` (`subtitle`, `value` (selectable, one line), `badge`, `leading`, `leading_tone` (draws the leading icon in the accent, warning or danger colour, for a row that reports a state), `leading_space` (a blank icon column so text lines up), `leading_app`, `navigates`, `selected`, `destructive`, `context_menu`, `on_press` (makes the row activatable)), `radio_row(title, selected, on_select)` (a radio on the leading edge; the whole row selects; `subtitle`, `value`, `leading_app` as for `row`), `radio_rows(title, rows)` (a group of radio rows: Up and Down move the selection, and only the selected row is in the Tab order), `switch_row` (`nested` for sub-rows shown while on), `slider_row` (a `level_row` for a setting), `level_row` (title on the left and a slider filling the rest of the line; `control` such as a mute button sits between them, `footer` under the track, `mark` and `mark_label` for a tick such as 100%, `danger_from`, `value_text`, `max`, `leading`, `context_menu`), `choice_row`, `text_row`, `entry_row`, `expander_row`, `property_row` |
| Icons            | `icon` (bundled symbolic glyphs), `app_icon` (full-colour application icons from the icon theme)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Groups and text  | `Group` (`description`, `content` for a chart or other free content), `section_label`, `caption`, `centered_caption`, `action_row`, `heading`, `status` (loading, empty, error, info blocks)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Path text        | `path_label(path)` (one line that ellipses a long path in the middle, keeping the start and the end: `~/Documents/…/Reports/final.pdf`; `style`, `ink`, `name` for a file name, `middle` for other text, `selectable`, `copyable`; see Path text below). `elide_path`, `elide_name`, `elide_middle` are the same cuts as pure functions of a width and a measure function |
| Layout           | `hstack(Space)`, `vstack(Space)`, `spacer`, `divider`, `card`, `clamp(Dim, child)`, `page_column`, `tile_rows(tokens, tiles, height)` (two tiles a row on a phone, four on a desktop)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Shell            | `NavigationSplitView` with `NavItem` and `subpage`, `HeaderBar`, `WindowControls`, `status_page`, `banner`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Page header      | `PageHead` (`subtitle`, `action`, `switch`): the title block that starts each page's content, with a status line under the title and one action or master switch at the right                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Search           | `NavigationSplitView::search(query, results)`: while the sidebar search has text, `results` replaces the page. Ctrl+F focuses it and Escape clears it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Search highlight | `search_highlight::provide(query)` in the results pane, then `search_highlight::label(style, tokens, text)` or `paragraph(...)` for text below it: each matched word gets an accent tint. Outside a search, or with no match, the text draws exactly as `TextStyle` would. While a search has text, a `Row` whose text does not match renders nothing, and a `Group` with no visible row hides with its title. `Group::unfiltered()` keeps every row for a group whose rows matched by other text, such as an index entry                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Overlays         | `dialog` with `ResponseKind::{Default, Suggested, Destructive}`, `framed_dialog(title, content, width, height)` (a dialog that is only its content, for a preview or a properties sheet), `Overlays::toast`, `Overlays::toast_with_action`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Popups           | `popup(title, content)` with `Popup::size`, `Popup::tabs` and `Popup::address`, opened by `window.open(popup)` from `use_window()`. A popup is a modal panel of its own, centred in the window: a Cancel button on the left, the title in the middle, the primary action on the right. It is 600 px tall unless `Popup::size` says otherwise, and it is clamped to the window. The body declares the primary action with `use_popup_actions(PopupActions { primary: Some(PopupAction { .. }) })`, and the caller closes the popup with `window.close()` once its work succeeds. Build the body with `popup_column()` and put each block in a `Group`, so the cards stand out from the panel. Cancel and Escape close it; Enter runs the primary action. The panel grows to at most 80% of the window height, and the body scrolls past that                                                                                                                                    |
| Forms            | `form_row(label, control)` for a label beside a control; `form_field(title, value)` for a validated text row (see Forms below); `live_state(rows)` for a read-only "Right now" group; `pending_bar(dirty, valid, on_apply, on_revert)` for the apply bar of a form with a draft; `segmented(options, selected, on_select).fill()` for a popup tab rail of equal segments (it stays a rail on a phone) |
| Trees            | `tree_view(roots, expanded, selected)` takes `TreeNode`s (`id`, `label`, `detail`, `icon`, `badge`, `children`) and the sets of open branches and the selected id as state. Rows are `MenuItem` height. Up and Down move the selection, Right opens a branch or enters it, Left closes it or goes to the parent, Enter and Space activate. A click selects; clicking the selected row activates it. `TreeView::on_hover` and `TreeView::on_activate` report to the caller                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Splits           | `split_pane(first, second, ratio)` with the first pane's share of the length in `ratio` (kept between 0.15 and 0.85). Side by side by default; `.vertical()` stacks the panes. Drag the divider to resize; the divider is a hairline with an `S1` hit zone on each side                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Menus            | `menu_item` and `menu_button(label, items)`. `MenuItem::checked` (state is a tick; menus are text, and `MenuItem::icon` is only for a menu whose rows the icon names, never in the file views), `menu_button` opens its menu against its own button (below it, above when there is no room, edge aligned, kept inside the window), `Overlays::show_menu` at a point is for context menus only; `MenuItem::destructive` and `MenuItem::disabled`; `menu_separator()`. `Overlays::show_menu` opens at window coordinates and stays on screen: it opens upward or leftward when there is no room. Picking an item closes the menu, then runs it; Escape and an outside press close it. While a menu or dropdown list is open, the window behind it takes no presses, wheel turns or hover, and the wheel never scrolls the page behind a list                                                                                                                                                                                                                                                                                                                                                                                                                                                |

Every control supports keyboard use: Enter and Space activate. Arrows move
sliders and steppers. Page Up and Page Down move sliders and steppers by ten
steps, and Home and End go to the ends. Open dropdown menus take arrows, Page
Up, Page Down, Home and End, and typing searches their options (see Type-ahead
below). Enter, Space and the arrows open a closed dropdown. Escape closes
dropdowns, menus, popups and dialogs. Segmented controls and radio groups are
roving: only the selected item is in the tab order, and the arrows move the
selection and focus together (Up and Down also work in a vertical radio group,
wrapping at the ends). Text fields: Enter submits, and Escape reverts an edited
value and then leaves the field. A password field has a reveal button, and a right
click opens Cut, Copy, Paste and Select all. The text cursor shows over the
editable text only: the reveal and clear buttons, the search glyph and any
suffix show the normal pointer, as every other control does. On touch, a tap places the caret,
a second tap on the same spot, or a tap in an empty field, shows a compact edit
bar over the caret (Paste while the clipboard holds text, Select, Select all;
Cut, Copy and Paste once there is a selection),
and a double tap or a long press selects the word under the finger. Two caret
handles mark a selection and drag to resize it; the bar, centred over the
selection, follows it. The bar's chevron opens the full menu as a floating
panel against the same caret or selection, never a sheet, and the selection
stays highlighted while it is open. Typing, Escape, a tap elsewhere or a scroll
hides the bar. Select all keeps it. Apps can append items to the
menu with `text_field(..).edit_menu_extra(items)`. In a popup, the panel takes
focus on open unless a field is marked `autofocus()`, and Enter in a
single-line field runs the primary action. Each control has an accessibility
role and a focus ring.
Animations are short (80 to 200 ms) and stop when `BINGUX_REDUCED_MOTION=1` is set.

### Split buttons

`SplitButton::new(icon)` is a primary action beside a chevron that opens a menu: the view button of a file manager, whose icon switches the view and whose menu picks one. It follows the Bingux interaction layers and the nested radius rule.

```rust
SplitButton::new(Icon::ViewList)
    .tooltip("Switch view")            // the primary segment's tooltip and name
    .menu_label("View options")        // the chevron's accessible name
    .on_press(move |_| toggle_view())  // the primary segment
    .menu(vec![                        // the chevron opens this, against the whole control
        menu_item("List", move |_| set_view(List)).checked(is_list),
        menu_item("Grid", move |_| set_view(Grid)).checked(!is_list),
    ])
```

| Part | Behaviour |
| ---- | --------- |
| At rest | The icon, a hairline and a chevron, with no fill. The control is `Dim::Button` tall. |
| Pointer over | The whole control becomes one `Radius::Control` box with the hover layer, and the hairline fades out. The segment under the pointer takes a second hover layer, or the press layer while down. The segments sit `S1` inside the box, so their corners are `Radius::Inner` (the box radius less that inset). |
| Keyboard | Tab reaches the primary segment, then the chevron, each with its own focus ring. Enter and Space press the focused segment (on the chevron they open the menu). Down on either segment opens the menu with the first item highlighted; Escape closes it and focus returns to the segment that opened it. |
| Menu | The same menu and placement as `menu_button` and `dropdown`: below the control, its edge aligned with the control's, above when there is no room, and held inside the window. A press on the chevron while the menu is open closes it. A mouse opens it on press, a touch on lift. |
| `on_menu(handler)` | The chevron runs `handler` instead of opening a menu, for an app that draws its own popup. Without `menu` or `on_menu` the control is the primary segment alone. |
| `label(text)` | Shows text after the icon; the segment grows to fit and the text is its accessible name. Otherwise the segment is square and `tooltip` is the name. Set one of them: a segment with no name fails the inspector's accessibility check. |
| `enabled(false)` | Dims the control and ignores the pointer and keys; both segments stay in place. |

On touch every size token takes its compact value, so the control is `Dim::Button` (48 px) tall.

### Path text

`path_label(path)` shows a path on one line and, when it does not fit, cuts it in the middle as the macOS Finder does, so you see where it starts and where it ends. Whole middle segments go first, from the middle outward, behind one `…` segment. The first segment (`/`, `~`, a mount name or `smb://host`) and the last stay. Only when `first/…/last` is still too wide is the last segment shortened in its middle, keeping its extension (`final-rep…2026.pdf`), and finally the first. The text is never empty: the least is `…`.

```rust
path_label("/home/kieran/Documents/Reports/final.pdf")
    .style(CAPTION)
    .copyable(true)      // right click: Copy path

path_label("final-report-2026-edition.pdf").name()   // keeps ".pdf"
```

The label fills the width of its parent and is one line tall, so a cut never moves anything around it. It measures the text with the font and size it draws in, redoes the cut when the width changes, and shows the whole text on its first frame before it has a width. While the text is cut, the full text is the hover tooltip. `.copyable(true)` adds a right click (or long press) item that copies the whole text, not the cut one. `.selectable(true)` lets the pointer select the shown text and Ctrl+C copy it; use it in a properties dialog, not in a list row, where a selection would take the press that opens the row.

To decide a cut in your own layout, call `elide_path(path, max_width, &|text| width_of(text))`, `elide_name` or `elide_middle`. A wider `max_width` never gives a shorter result.

### Forms

`form_field(title, value: State<String>)` is a `Group` row for one text input. The title, with an optional hint line under it, sits at the left and the field at the right; on a phone the title and hint are above and the field fills the row. An error shows under the field in the danger colour, announced to assistive technology as an alert, and the field gets the danger outline. A field that can fail keeps a blank message line, so the row never changes height when a message comes and goes.

```rust
use bingux_ui::controls::form::validate;

Group::titled("Server")
    .row(form_field("Address", address).hint("Host name or IP address").validate(validate::hostname_or_ip))
    .row(form_field("Port", port).suffix("TCP").validate(validate::port))
    .row(form_field("Key", key).monospace().multiline(3).optional())
```

| Method                   | Effect                                                                                           |
| ------------------------ | ------------------------------------------------------------------------------------------------ |
| `hint(text)`             | One muted line under the title.                                                                  |
| `placeholder(text)`      | Greyed text in the empty field.                                                                  |
| `password()`             | Masks the text; the field's reveal button shows it.                                              |
| `monospace()`            | Monospace face for keys and addresses.                                                           |
| `wide()`                 | `Dim::FieldWide` (440 px) on a desktop, for a key or URL. Give every field of a group the same width. |
| `suffix(text)`           | A muted unit inside the field's right end, such as `bytes`.                                      |
| `trailing(element)`      | An element after the field on the same line (phone too), such as a remove `icon_button`.         |
| `optional()`             | The muted word "Optional" after the title.                                                       |
| `multiline(lines)`       | A field `lines` lines tall; Enter adds a line.                                                   |
| `error(Option<String>)`  | Shows a message now, touched or not, for example after a failed Apply. It wins over `validate`.  |
| `validate(fn)`           | Runs a check from `form::validate`, or your own `fn(&str) -> Result<(), String>`.                |
| `validate_with(closure)` | The same with a closure, such as `validate::integer_in(576..=9000)`.                             |
| `on_submit(f)`           | Enter in a single-line field.                                                                    |
| `autofocus()`            | Takes focus when it first appears; only one field per page.                                      |

`validate` never shows a message on an empty field or on one the person has not left: the message appears once the field is non-empty and has lost focus (or Enter was pressed in it), and from then on follows every keystroke, so it goes as soon as the text is right. The check is for the field only: the form runs the same function again to decide whether Apply is allowed. The validators are pure functions returning a plain-language message; `form::validate` has `ipv4`, `ipv4_cidr`, `ipv6`, `ipv6_cidr`, `ip`, `ip_list` (commas or spaces), `ipv4_netmask_or_prefix`, `mac`, `port`, `hostname_or_ip`, `url`, `wireguard_key`, `ssid` (bytes, not characters), `wpa_passphrase` and `integer_in(range)`. An empty string is an error for every validator, and an IPv6 zone such as `fe80::1%eth0` is rejected.

`pending_bar(dirty, valid, on_apply, on_revert)` returns `Some` only while `dirty`: a group holding "You have unsaved changes", Revert and Apply (disabled while `valid` is false). Put it at the top of a form with `.maybe_child(..)`. `live_state(vec![(label, value)])` is a "Right now" group of read-only rows with monospace, selectable values, to show the effect of what the form sets (address, gateway, DNS, link speed).

Outside a form, `text_field` takes the same looks: `.monospace()`, `.lines(n)` (the height of a `.multiline()` field), `.suffix(text)` and `.danger_outline(bool)` (the outline without the message line). `row(..).mono_value(text)` is `value` in the monospace face, selectable like it. `.leading_chip(label, remove_label, on_remove)` puts a removable token (such as the `in Downloads` scope of a search) inside the field after the leading glyph; it never changes the field's size. `.on_enter_modifiers(f)` reports Ctrl, Shift and Alt just before Enter submits, so a field can tell Enter from Ctrl+Enter.

Menus take the keyboard too. Enter, Space or Down on a menu button opens its
menu with the first item highlighted. Up and Down move the highlight over
enabled items and skip separators; Home and End go to the first and last item.
Left and Escape step back one level; Escape at the top level and Tab close the
menu and return focus to the control that opened it.

Every row of a menu answers the pointer: hover or keyboard focus highlights it.
A disabled row is not lit, but it still counts as a move onto a sibling, so the
pointer never crosses a dead zone. A panel with a tick (`MenuItem::checked`) or
an icon on any row keeps one leading column for all its rows, so labels line
up; radio groups are rows with a tick as well. An item made with
`submenu(label, items)` opens its submenu on hover after 150 ms (at once when
another submenu of the same menu is already open), on Right, Enter or Space, and
on a click or a tap, and it opens to the left when there is no room on the
right. Moving to another row closes it. A pointer moving from an open row toward
its submenu keeps the submenu open while it stays inside the safe triangle
between the pointer and the submenu, and a rest of 300 ms in the triangle ends
it. The pointer and key rules are `shell/menu_nav.rs`, a pure state machine
with an injected clock; `menu.rs` only draws it.

**Type-ahead.** Typing while a menu, dropdown list or crumb folder list is open
does not close it. The letters build a query that starts again after a second
without typing; Backspace edits it. A small pill under the list (above it when
there is no room) shows the query and how many rows match, and is announced to a
screen reader as a polite status ("3 matches for fol"). A row matches when its
label contains the query, ignoring case; a label that starts with it ranks first,
then one with a word that starts with it, then any other substring. The best
match takes the keyboard focus and the matched letters of every match are marked
on the accent tint, while rows that do not match are dimmed (nothing is removed,
so the list does not reflow). While a query is active, Down, Up, Tab, Shift+Tab,
Ctrl+N and Ctrl+P walk through the matches only, wrapping at the ends, Enter
picks the focused match, Space types a space (it picks as usual when there is no
query), and Escape clears the query before a second Escape closes the list.
Disabled rows and separators never match. The rules are `shell/menu_query.rs`
(pure, with an injected clock); `shell/type_ahead.rs` has the window side. A
list that keeps its own highlight uses `use_list_query`, `ListQuery::key`,
`marked_label`, `dimmed` and `Panel::pill`; a closed dropdown still chooses the
best match as you type. Escape is heard by the panel anywhere in the window, so
a list's own key handler leaves it alone and gives the panel `Panel::on_escape`
(`ListQuery::escape` clears the query, else the list closes).
`search_highlight::highlighted_label(style, tokens, text, ranges)` draws a
label with byte ranges marked, for any list of names.
`MenuItem::shortcut("Ctrl+C")` shows a muted hint at the trailing edge; the app handles the keys. A menu panel with any hint is `Dim::MenuWide` (300 px) instead of `Dim::Menu` (220 px).

Popups share one motion, all with an ease-out curve and all scaled by
`tokens.motion(ms)`, so they are instant under `BINGUX_REDUCED_MOTION=1`.
Each layer stays on screen until its exit finishes.

- Popup: fades in and scales from 94% to 100% in 150 ms. The scrim fades with it. Closing reverses it.
- Dialog: fades in and scales from 96% to 100% in 120 ms. The scrim fades with it.
- Menu: fades in and scales from 96% to 100% in 90 ms, pivoting on its top-left corner (the anchor).
- Toast: slides up 8 px and fades in over 140 ms. Dismissing it slides it back down and fades it out.
- Dropdown: on a phone a sheet (see Panel); on a desktop it fades and scales in over 90 ms, the same easing as the menu, anchored to its button.
- Subpage: a push slides the new page in from the right over 320 ms on an expo ease-out, and the page beneath stays still; a pop slides it back out to the right. The moving page is a card while it moves: its leading corners take `Radius::Window`, and a soft shadow (`sheet_edge_shadow`: 24 px offset, 32 px blur, black at 0.5 alpha at full amount) falls on the page beneath. Both scale to nothing over its last 15 %. The window's own buttons stay at the window's top right while it moves. A sidebar switch cross-fades over 120 ms. A move that starts mid-slide continues from where the slide is.
- Sheet (a popup on a narrow window): slides up from the bottom. With `PopupFlags::GRABBER` it can be dragged down by its grabber; released past `SHEET_DISMISS_DISTANCE` (96 px) it dismisses, otherwise it springs back.

A popup with `PopupFlags::INSET_VIEWPORT` shrinks the window behind it into a rounded card on black: the window scales to `INSET_SCALE` (0.96) and its corners take the `Card` radius, both in step with the popup's open and close. On a sheet the inset also follows the pull: full at rest, it eases down in proportion to the drag and reaches zero at the dismiss distance, then springs back with the sheet on release. Desktop popups do not inset.

### Panel

`shell/panel.rs` is the one floating-surface primitive: context menus, dropdown lists, popovers, sheets, popups and the text edit bar are all `Panel`s. A variant is configuration, not a new component. A panel owns its backdrop (a shield that takes presses, wheel and hover outside it), presence, placement (flip and clamp inside the window), dismissal (Escape, an outside press, an address change), focus trap and restore, and roving keys.

A panel knows its trigger: `PanelTrigger::Pointer`, `Control`, `Row`, `Text` or `Keyboard`. Placement is measured from that trigger.

Phone rule: a panel is a sheet only for choosing a value from a set (a dropdown such as Resolution or Refresh rate) or a long titled list. A sheet slides up with a fading backdrop, has a drag handle, a height cap (a token, about 60 to 70 percent), a header with the control's label, and a scroll affordance. It opens with the selected item visible. A floating panel is for actions (row long-press and context menus, overflow menus): anchored to its trigger, no dim, and an outside tap dismisses it. A text trigger uses the text edit bar. On a desktop everything floats, and popups are centred.

The shield is the only input shield; there is no second copy of the shield or grabber code.

## Files and the file chooser

`bingux_ui::files` is the SDK's answer to "let the person pick a file". An app
never shells out to `zenity` or `kdialog` and never builds its own picker:

```rust
use bingux_ui::files::{ChooserRequest, Filter, choose};

choose(
    use_window(),
    ChooserRequest::open()
        .title("Import VPN configuration")
        .filter(Filter::new("VPN configuration").glob("*.conf").glob("*.ovpn"))
        .filter(Filter::new("All files")),
    move |chosen| {
        // Runs once, on the UI thread. `None` means the person cancelled.
        if let Some(chosen) = chosen {
            import(&chosen.paths[0]);
        }
    },
);
```

`ChooserRequest::open()`, `select_folder()`, `save(name)` and
`save_files(names)` start a request; `title`, `accept_label`, `multiple`,
`filter`, `filter_spec`, `filters`, `presets`, `current_filter`,
`current_folder`, `current_file`, `choice` and `view(ViewMode::{Auto, List,
Grid})` refine it. The answer, `Chosen`, holds absolute `paths`, the active
`current_filter` and the `choices`. The dialog is a popup: a panel on a
desktop window, a full sheet on a phone (Cancel, the accept button,
`PopupFlags::FILL`).

What the dialog has:

| Part | Behaviour |
| ---- | --------- |
| Toolbar | Back, Forward, Parent folder (a folder with an arrow out of it), Refresh, the address bar, the file type dropdown, New folder, a list or thumbnails switch and the view menu (sort, hidden files). Every button follows the `Control` radius like any other button, and the view menu opens against its own button, not at the pointer. Refresh (also F5 and Ctrl+R) reads the folder again even when no change was seen, as a network or removable drive sends none; the selection, scroll position and filter stay, and the button shows a spinner of the same size while it reads. |
| Address bar (`AddressBar`) | The path as crumbs, and the search box (see [Search](#search-in-the-file-views)). The field takes the `Control` radius and a 4 px padding, so each crumb inside it takes the `Inner` radius (12 less 4). Each crumb has a chevron that lists that folder's subfolders. A trail that does not fit keeps its first and last crumb and hides the middle behind one `…` menu. Click the empty end, press Ctrl+L, or type `/` or `~` to edit the path as text. Typing shows completions (folders first); Tab completes, Up and Down choose, Enter opens, Escape closes. Errors show inline: not found, not a folder, no permission, not responding. `~`, relative paths and `file://` work. |
| Sidebar (`PlacesSidebar`) | Recent, Home and the user folders (with their own icons by path), Bookmarks, then Drives: Computer and every mount, each with a usage bar and "X free of Y". A drive that does not answer says "Not responding". There are no buttons on the rows: every row has a context menu (right click, long press, the Menu key or Shift+F10) of text items. A bookmark offers Open, Open in new window, Rename… and Remove from bookmarks; a drive Open, Unmount or Eject, Add to bookmarks, Copy path and Properties. The folders have their own icons: a folder with a page for Documents, a down arrow for Downloads, and so on. A phone gets `PlacesMenu`, a sheet with the same rows. |
| File type filter | A dropdown in the toolbar on a desktop; on a phone a "File type" row at the bottom. The sidebar runs the dialog's full height: there is no footer under it, and the name field of Save, the request's choices and any problem sit in a strip at the bottom of the content pane. With no filters in the request it offers All files, Images, Audio, Video, Documents, Archives and Text. `presets(true)` adds those after the request's own. The list shows only matching files (folders always stay) and the status pill says how many the filter hides. The pill floats over the bottom-left corner of the list and takes no row; when the pointer reaches it, it moves to the other corner. |
| New folder | The toolbar button, Ctrl+Shift+N and the empty-space menu make a folder and start renaming it in the list, with the name checked as you type. |
| View | `FileView`: columns, zoom, rubber band selection and context menus. Each folder remembers its icon size and sort. |

**The file view.** `FileView::new(listing, order, selection)` shows a folder as a list with columns or a grid of tiles, chosen by the zoom level. List columns: Name, Size, Type, Modified, Created, Accessed, Permissions, Owner (Location in Recent). Drag a column's left edge to resize it (double-click fits it), drag a heading to reorder, right-click the header to show or hide columns; widths, order and visibility are remembered in `$XDG_STATE_HOME/bingux/files/view.json`. A row is the sidebar's row (`LIST_ITEM`: the same radius, inset, fills and minimum height), held `LIST_EDGE` clear of the list's sides, and the header lines up with it. **Zoom** changes the icon and nothing else: the text, its size and the column widths never change. The level is an icon size in pixels from one ladder, `16, 20, 24, 32` (a list, and 16 px is the default in every folder) and `48, 64, 96, 128, 192` (a grid of tiles with the name under the icon in the same text size, each tile growing with its icon). Ctrl+wheel, Ctrl+plus and minus step one level, Ctrl+0 returns to 16 px in a list, a pinch picks the nearest level, and a badge shows the size and view for a moment. Choosing a list or a grid (toolbar, View as menu, Ctrl+1 and Ctrl+2) goes to the nearest level of that view. Nothing picks a larger level for you, neither the window size nor what the folder holds; the one limit is that a tile must fit twice across, so a narrow view stops at a smaller level. Images, and videos and documents that have a desktop thumbnail, show their picture, uncropped, at every grid level and in a list from 24 px; folders and everything else keep their icon. The level is remembered per folder in the same file (`icon_px`). Right-click gives a menu for a file, several files, empty space and the headers; Menu or Shift+F10 opens it from the keyboard. Rename in place with F2, rubber-band from blank space, drag files onto a folder (Ctrl copies), hover for a detail card. Rows are drawn only when in view, sizes and dates are asked of the loader only for those rows, and thumbnails load after a short rest in view and never on network drives. Keys: Space (Quick Look), Ctrl+A, Ctrl+Space, F2, Delete, Shift+Delete, Ctrl+C, X, V and Z, Ctrl+Shift+N, F5, Alt+Enter or Ctrl+I (properties), Ctrl+1 and 2 (list, grid), Ctrl+Alt+1 to 4 (sort) and Ctrl+Alt+R (reverse).

**Context menus of the file view.** A menu is grouped by task and holds only what can apply: a row that can never apply in that place is left out, not greyed. The empty-space menu is, in order: New folder, New file (a submenu of Empty file and the files of the XDG Templates folder when that folder has any, otherwise one row); Paste (greyed until something is copied, left out in Recent) and, only when there is something to take back, a named Undo such as "Undo Rename" (there is no Redo); Select all and Invert selection (left out in an empty folder); one View submenu; then Refresh and Properties. View holds List and Grid, Icon size (the sizes of the view on show, from the zoom ladder), Sort by (Name, Size, Type and Modified; Ascending and Descending; Folders first) and Show hidden files. Choices are ticked rows in one leading column, so labels line up. The header menu's Sort by is the same submenu. Shortcuts show on the right.

**Quick Look.** Space on the selected file opens a large card over a dimmed scrim, as macOS Quick Look and GNOME Sushi do; the context menu has Preview too. Space or Escape closes it; Left and Right (Up and Down too in a grid, and in a list they step one file) move the selection to the neighbouring file and update the card without closing it; Enter closes it and opens the file as a double click does; Alt+Enter or Ctrl+I closes it and shows Properties. Space only opens it when the file view itself has the keyboard: never while a rename field, the address bar or a filter has it, and not in the middle of a word typed to jump to a file. It works in the chooser dialog as well. What it shows depends on the type: a picture scaled to fit with its proportions kept; the first 64 KiB of a text, code, markup or JSON file in a monospace face, with a notice that says how much is shown when the file is longer; the largest thumbnail the desktop has made for a PDF, document, song or film, with the type's icon when there is none; a folder's item count and total size, counted while the card is open; an unknown type as text if it is text, else as its icon. The footer says the type, size, picture size or length and codec, and date. Everything loads off the UI thread. The stage is a fixed box and the type's icon is the placeholder at the size the content will have, so nothing moves when the content arrives, and stepping to another file drops the load of the last one (a folder count stops after its current entry).

**Properties.** Alt+Enter, Ctrl+I or the menu's Properties row opens a dialog for the selection, or for the folder on show when nothing is selected. For one file: its picture or icon, name and type; type with its MIME type; size with the byte count (a folder or a selection is added up on a thread, with a Stop button, and the figure grows while it runs); location (press it to open that folder); where a link leads; for pictures the size in pixels and, for JPEG, the camera, lens, date taken and exposure from its EXIF; for audio and video the length and codecs (WAV, FLAC, Ogg Vorbis and Opus, MP4, M4A and MOV, Matroska and WebM; the length of an MP3 and the details of SVG, HEIF, AVIF, TIFF and JPEG XL are not read); created, modified and accessed times; owner and group; permissions as a grid of read, write and execute boxes for the owner, group and others, which the owner (and root) can change with chmod on a drive that is not read-only, with the numeric mode and "Allow executing as program"; the drive with its name, device, mount point and a free space bar; and, in a view that can open files with another application, the applications that open the type. Several items show their combined count, size and types, where they are, and the drive. Every row exists from the first frame, with "…" until its fact arrives and "—" when it has none, so nothing moves while facts load.

**Moving, copying and removing safely.** The file operations are `bingux_files::ops`; the views call nothing else.

| Operation | What it does | What it promises |
| --------- | ------------ | ---------------- |
| Rename, and a move on one drive | One `renameat2(RENAME_NOREPLACE)` | Atomic. Nothing is copied or written. A name that is taken is never replaced unless you chose Replace. |
| Move between drives | Copy, flush, check, then remove the source | The source stays until the copy is durable and checked: every copy is as long as what was read, and nothing changed while it was read. A failure, a cancel, a full drive or a pulled cable leaves the source exactly as it was. Items you skipped are never removed. |
| Copy | Reflink (`FICLONE`) where the file system has one, else `copy_file_range`, else a 4 MiB buffer | Holes stay holes (`SEEK_DATA`/`SEEK_HOLE`), so a sparse disk image stays sparse. A full drive fails at the start of a file, not at the end. Many small files are copied by up to 8 threads (4 on removable media and shares); a big file is copied alone. Mode, times and extended attributes are kept as far as the drive has them (FAT has no modes and says so, which is not an error). |
| Any write | Data goes to `.bingux-*.part` beside the destination and is renamed into place | A half-written file never has its real name. A failed or cancelled copy removes its part files. Replacing swaps the new file in whole: the old one is intact until then. |
| Delete, rename and new folder | A single call | On removable media the folder is written out before the call returns. |

**Removable media.** A copy, move or delete on a USB stick, SD card, external disk, network share, or a FAT, exFAT or NTFS drive is only *done* when the data is on the device. The drive is recognised from the mount table, the device's `removable` flag and a USB bus in `/sys` (also under an encrypted layer). For those drives every file is `fsync`ed before it gets its name, then every folder that changed, then the whole drive is written out (`syncfs`). While that last step runs the status line says **Finishing… writing to <drive>** with a bar that keeps still and no Stop button; the line that follows says **Everything is written to <drive>.** An ordinary internal disk is not flushed: that is the kernel's job, and flushing would only make copies slower. **Eject** (and Unmount) in the sidebar write the drive out first, then unmount; if the drive is busy it says so in the tool's own words, and when it is ejected the toast says the drive is safe to remove. A drive that cannot be written out is not detached.

**Removing.** Delete and *Move to Trash* put the item in the Trash: `$XDG_DATA_HOME/Trash` on the home drive, `$topdir/.Trash/$uid` or `$topdir/.Trash-$uid` on another drive. It is one rename, and Ctrl+Z or the toast's Undo puts it back. *Delete permanently* is **not** in the ordinary context menu. It appears when you open the menu with Shift held (Shift and right click), and Shift+Delete does it from the keyboard. Both open a question first: it names the item (or says "3 items"), says that it cannot be undone and will not go to the Trash, has a red **Delete permanently** button, and gives the focus to Cancel, so Enter keeps the files. Something the Trash cannot take (a drive with no Trash, a network share, a read-only drive, or something in the Trash already) is never deleted quietly: the same question opens, with the reason. **Empty Trash** (in the empty-space menu while a Trash folder is on show) asks too. Rename, move and Move to Trash can be undone; deleting for good cannot, and the question says so.

A filter takes file extensions, globs and MIME types in one string:

```rust
ChooserRequest::open()
    .filter_spec("Pictures", "png jpg image/webp")   // extensions and a MIME type
    .filter_spec("Documents", "*.pdf *.odt")          // globs
    .presets(true)                                     // and the built-in ones
```

Keyboard: Tab walks the toolbar, list and the strip under it. In the list the arrows, Home,
End and Page keys move (Shift extends when several files are allowed), typing a
name starts a search of the folder (see [Search](#search-in-the-file-views)),
Ctrl+A selects all, Enter opens. Ctrl+L, or typing `/` or `~`,
edits the path as text. Backspace or Alt+Up goes to the parent folder, Alt+Left
back, Alt+Right forward, Alt+Home home, Ctrl+H toggles hidden files, and Escape
cancels. On touch a tap opens a folder and selects a file.

Folders at the XDG user directories (Documents, Downloads, Music and so on)
get their own icon. It is decided by the path alone, as the person sees it:
`~/Downloads` keeps the Downloads icon when it is a link to a share, and the
share's own folder, reached by its real path, is a plain folder. No call
touches the disk, so it costs nothing on a network mount.

### Search in the file views

The address bar is the search box. One text field takes a path or a search, and
it decides from what is typed, without touching the disk:

| Typed | Is |
| ----- | -- |
| starts with `/`, `~`, `./`, `../`, or a scheme such as `smb://` | a path |
| the name of an entry in the folder (`Documents`, `Documents/Sub`) | a path |
| anything else (`report`, `2024 invoice`) | a search |

The bar has three states that share one frame and one height, so nothing moves
when it changes: the **trail** of crumbs, the **path** field with folder
completions (Ctrl+L), and the **search** field, which has the search glyph and
a chip `in <Folder>` inside it. The chip's close button, or Escape, ends the
search. Going to another folder ends it too.

**Filter this folder.** Ctrl+F, or typing a letter on the list, focuses the bar
in search mode (the list's jump-to-a-name is replaced by this). The loaded
listing is filtered as each letter is typed; nothing is read from the disk.
Matching ignores case and looks at names only, in this order: the exact name,
a name that starts with the text, a word that starts with it, the text
anywhere, every word of it in any order, and last the letters in order (`rpt`
finds `my_report.txt`, for three letters or more). The list keeps your sort
inside each of those groups. Matched letters are drawn on an accent tint, and
the status pill reads "3 of 120 items match". Up and Down move the selected
match without leaving the field, Enter opens it (a folder is entered, a file is
opened or chosen; with no match it searches the subfolders), and Escape clears
the text, shows the whole folder and gives back the selection you had before.
Clicking a result keeps the filter until Escape.

**Search the subfolders.** While there is text, the last row under the field is
always *Search subfolders for "text"*; Ctrl+Enter or Shift+Enter does the same.
Results stream into the list as a background walker finds them, shallow folders
first, with a live count in the row and the status pill; the row says **Stop** and
pressing it ends the walk (what was found stays). Changing the text, going to another folder or
closing the dialog cancels the walk. Each result shows its name and, in a list,
a **Location** column with its folder below the folder searched (a grid shows it
on the detail card, and a phone's list on the second line). The walker:

- reads one folder at a time on one thread, breadth first, and stops at
  10,000 results and says so;
- skips hidden folders and hidden files unless *Show hidden files* is on;
- never follows a symbolic link, never enters another filesystem (a mounted
  drive or network share, `/proc`) and never enters a folder twice;
- looks in everything you could browse to, including `node_modules`, `target`
  and `.git` (when hidden files show), because the view menu's **Search build and
  dependency folders** switch is on by default. Turn it off to skip
  `node_modules`, `.git`, `.hg`, `.svn`, `target`, `__pycache__`, `.venv`, `.tox`,
  `.gradle` and `.cache`. The switch lasts for the dialog.

The Bingux search service (`bingux-searchd`) is not used: it is a launcher
search over a fixed set of roots, with results capped at the launcher's size,
over a private socket; it cannot be limited to a folder or say which folders it
covers. File **contents** are not searched, only names.

The file type filter of a chooser is a separate thing. It stays on during a
search, so searching a "Pictures" chooser finds only pictures (folders still
appear). In the folder, its "hidden by filter" count is reported on its own; in
the subfolders the walker applies the filter before it counts, so rejected files
neither show nor use up the 10,000 results. In the model
(`bingux_files::search`) a search is a `Query`; a filter is a `Filter`; the two
never share a type. Apps build the same behaviour with
`AddressBar::search(SearchLink::new(find, selection, folder)...)`,
`FileView::search(..)` / `.on_search(..)` and `use_subfolder_search(..)`; the
chooser is the reference.

### Layers

| Layer | Where | Depends on |
| ----- | ----- | ---------- |
| Filesystem model: listing, MIME, places, bookmarks, recent files, filters, name search and the subfolder walker, sort, inotify watching, selection, request and answer types, what accepting does | `packages/bingux-files` (crate `bingux-files`, `libc` only, unit tests) | nothing |
| Views: `PlacesSidebar`, `PlacesMenu`, `AddressBar`, `FileView`, `use_directory`, `use_places`, `empty_state`, `error_state` | `bingux_ui::files` | the model, bingux-ui controls |
| The dialog: `choose` | `bingux_ui::files` (also in the prelude as `files`, `ChooserRequest`, `Chosen`) | the views |

The model has no UI and no async runtime, so the future Files app, a command
line tool and a portal backend use it as it is. Listing blocks on the disk
and takes a `Cancel`; `bingux-ui` runs it on its worker pool and keeps the
folder current with an inotify watch (bursts merge, a changed folder lists
again without showing the loading state). MIME types come from a table of the
common shared-mime-info globs embedded in the crate: it needs no host files
and no dependency, and, like GIO's fast content type, looks at the name
only, never at file contents. The list builds only the rows in view, so a
hundred thousand files scroll as one hundred do, and the grid decodes each
thumbnail only when its row scrolls in. The views are ordinary components:
`PlacesSidebar::new(places, current, on_open)`,
`AddressBar::new(location, home, editing, draft, on_open, on_file)` and
`FileView::new(listing, order, selection).icon_px(level).sort(..).multiple(..)
.on_pick(..).on_activate(..)`. A second user of them is the Files app.

### Opening a folder fast, on any drive

`bingux_files::loader::open` is how every view opens a folder. It never
blocks the caller and sends events: `Kind`, `Cached`, `Batch`, `Done`, `Diff`,
`Stats`, `Unresponsive`, `Failed`. A view applies each with
`Listing::apply_event`, which returns true when it should select again, and
asks `Load::want_stats(&listing, rows_in_view)` on every frame.

| Step | What happens |
| ---- | ------------ |
| Cache | The folder's last listing is shown at once from `$XDG_CACHE_HOME/bingux/files/v1` (about 10 bytes an entry; 32 MiB and 512 folders at most, least recently used deleted first). One `statx` of the folder says whether it changed. If not, nothing else is read, locally or over NFS. If it did, only the difference arrives as `Diff`, so rows are patched and the scroll position and selection stay. |
| Streaming | With no cache the first 50 entries arrive after about 2 ms and the rest in batches that grow, so a 20,000-entry folder is complete in about 50 to 90 ms. |
| Drive kind | The kind (`Local`, `Removable`, `Network`, `Optical`, `Other`) comes from the folder's own device and the mount table, not its path: `~/Downloads` that links to an NFS share is `Network`. |
| Network drives | Names and types only, one worker, no thumbnails, no reading of file content (`Listing::mount.thumbnails()` is false). Size and time come only for the rows in view, newest request first. |
| Slow drives | A drive that stops answering gives `Unresponsive` after two to three seconds. The listing stays as it was and leaving the folder never waits. |
| Changes | inotify on local disks; a stamp check every five seconds on network ones. Both end as a `Diff`. |

Sorting and filtering 100,000 entries take tens of milliseconds
(`Listing::select`, `select_counted` also returns how many entries the filter
and the hidden setting left out). Filters are built from what an author writes:
`Filter::parse("Pictures", "png jpg image/webp")`, or the presets
(`Filter::images()`, `audio()`, `video()`, `documents()`, `archives()`,
`text()`); `ChooserRequest::effective_filters()` is the list the menu shows.
Drives, with size and free space, come from `bingux_files::mounts`
(`list_mounts`, then `fill_space` on a worker, `watch_mounts` for changes).
Which folders get the Documents, Downloads, ... icon is decided from the path
text only (`places::special_folder`): no `stat`, no `readlink`, so it costs
nothing on a network mount, and `~/Downloads` keeps its icon when it is a link
while the target folder, reached by its real path, shows a plain folder.
Timings: `packages/bingux-files/examples/bench.rs`.

### Portal mapping

`ChooserRequest` and `Chosen` map one to one onto
`org.freedesktop.portal.FileChooser`, so a later `bingux-contrib-portals`
backend (`org.freedesktop.impl.portal.FileChooser`) can host this dialog:

| Portal | Here |
| ------ | ---- |
| `OpenFile`, `SaveFile`, `SaveFiles` | `Action::Open` / `Action::SelectFolder`, `Action::Save`, `Action::SaveFiles` |
| `title` | `title` |
| `accept_label`, `modal`, `multiple` | the same names |
| `directory` | `Action::SelectFolder` |
| `filters` `a(sa(us))` | `filters`; `Filter::from_portal`, `Filter::to_portal`; rule kind 0 is a glob, 1 a MIME type |
| `current_filter` | an index into the effective filters: `filters`, or the presets when it is empty, or `filters` then the presets after `presets(true)` |
| `choices` `a(ssa(ss)s)` | `choices`; no options means a yes/no choice with `"true"` or `"false"` |
| `current_name`, `current_folder`, `current_file`, `files` | the same names |
| response `uris` | `Chosen::paths` through `location::file_uri` |
| response `current_filter`, `choices` | `Chosen::current_filter`, `Chosen::choices` |
| response code 1 (cancelled) | the callback gets `None` |

Globs ignore case, so `*.png` shows `A.PNG`. `ViewMode` is the one option the
portal does not have; a backend leaves it on `Auto`.

### Versions and the system service

`files::VERSION` numbers the compiled-in dialog and rises when a request option
or the dialog's behaviour changes. The design for sharing the dialog with other
apps: `bingux-contrib-portals`, a package that depends on `bingux-ui` and
`bingux-files`, runs the portal backend and hosts the same dialog body in its
own window, exposing its version as `org.bingux.Portals.Version`. `choose`
then picks the newest of the compiled-in dialog and the service: it asks the
service its version, calls the portal when that is higher, and uses the
compiled-in dialog when the service is missing, older or fails. The compiled-in
dialog always works. The package is not built yet: it needs a D-Bus client
(the `dbus` crate over libdbus is the only one in the offline cache; no
`zbus`) and a hosted native window.

Capture the dialog with `scripts/file-chooser-fixture.py DIR` and
`HOME=DIR cargo run --example file_chooser --features testing -- OUT` (every
mode at 1100x820 and 390x800). Add `--link-downloads` to the fixture script
to make `~/Downloads` a link to a folder outside the home. Set
`BINGUX_FILES_STATE=FILE` to keep your own remembered views out of the
captures. With `FILE_CHOOSER_LIVE=SCENARIO` the example opens a real window
that reopens the chooser after every answer, for driving with real input.

## Charts

`charts` draws sparklines, line and area charts, bar charts, rings and
donuts. Each chart fills the width it is given and keeps a fixed height from
its first frame, so data that arrives later never moves the layout. Before it
has points it shows a muted baseline and "No data yet" at the same height.
A single x value is centred with one tick at the value, not a -1 to 1 axis. A constant series sits in the middle of a window and labels its one value. Time axes span the data, not whole hours.

A chart is free content, so it has no padding of its own: put it in a
`boxed(tokens)` card, or on a group with `Group::content(chart)`, which keeps it
16 px from the card's edge on every side like any free content. (`Group::row` is
for rows, which sit flush against the card.) Inside the box, the y labels have
12 px of air before the plot, the x labels 8 px under it, and a legend sits
16 px under the x labels, lined up with the left edge of the plot.

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
| `sparkline(values)`  | A small trend with no axes. `colour`, `unit`, `format`, `area`, `label`.       |
| `line_chart(series)` | Several series over x. `y_unit`, `y_format`, `x_unit`, `curve`, `area`, `label`, `height`. |
| `bar_chart(series)`  | Grouped bars, or `stacked()`. `categories`, `details`, `y_unit`, `y_format`, `label`, `height`. |
| `ring(value)`        | A 0 to 1 gauge with a centre caption. `caption`, `colour`, `label`.            |
| `donut(segments)`    | A share of a whole, with a legend. `unit`, `caption`, `label`.                 |
| `timeline_chart(bars)` | Bars along a time axis, a limit line and a draggable range. |
| `flame_graph(tree)`  | Weighted calls as stacked bars, widest first.                                  |

`Series::new(name, points)` takes `(x, y)` pairs, and `Series::values` reads
values by position. `Segment::new(name, value)` is one slice of a donut.

`timeline_chart(bars)` takes `Bar::new(at, width, value)` in ascending order of
`at`. `x_range` and `y_max` fix the axes, `x_unit` and `y_unit` format them,
and `limit(value, name)` draws a dashed line and turns the bars over it red.
`selection(range)` and `on_select(handler)` make the range the user drags
yours to keep: the handler gets `Some((from, to))` when a drag ends and `None`
for a click or Escape. Left and Right walk the bars, and Shift with an arrow
selects.

`flame_graph(tree)` takes an `Rc<FlameTree>`. Build the tree with
`push(parent, name, weight)` (node 0 is the root), `add` to grow a weight,
`label` for the shorter text a bar shows, `highlight` to tint a bar in the
accent colour, and `sorted()` last. Bars show their label cut with an
ellipsis; the detail line beneath gives the full name and shares of the whole.
A click zooms into a bar, Escape zooms out, and the view starts past the calls
at the top that only call one thing. Down, Up, Left and Right walk the tree.

`Unit` formats values: `Number`, `Percent` (values already in percent),
`Bytes` (binary units, `3.2 GiB`), `DecimalBytes` (decimal units, `3.2 GB`,
with ticks on round decimal numbers), `Duration` (seconds as `2 h 15 min`) and
`Clock { offset }` (Unix seconds in local time). For anything else, `y_format(|value| ..)`
on a line chart, bar chart or timeline (and `format` on a sparkline) writes the
axis ticks and the tooltip values with your own function: `.y_format(|mbit| format!("{mbit:.1} Mbit/s"))`.
It receives round numbers for ticks and the data as it is for tooltips, so it
rounds for itself. `Curve` is `Linear`, `Step`
or `Monotone` (the default, a smooth curve that never overshoots).

Data handling: a `NaN` or infinite point breaks the line instead of joining
across it. Points are sorted by x, and a repeated x keeps its last value.

Motion: lines draw in from left to right, bars grow from the baseline and
rings sweep, over 700 ms. Under `BINGUX_REDUCED_MOTION=1` they appear at once.

Hover: a line chart draws a crosshair and a dot on each series at the nearest
x, and a bar chart fades the other bands, so the hovered bar stands out. A
tooltip card (the popover surface, `Raised` elevation) lists the x label, then
each series with its swatch, name and formatted value, and for stacked bars the
total under a hairline. It sits beside the stop on the side with room, flips
near an edge, and moves from stop to stop, never with the pointer, so it does
not jitter; it goes when the pointer leaves the chart. A bar chart's axis labels
thin to every few bands from the first, so they stay evenly spaced; give the
tooltip a longer heading than the axis label with `details(names)`
(`"14:00 - 15:00"` for a band the axis calls `"14:00"`). On touch, press and
hold shows the tooltip and sliding the finger moves it; lifting hides it, and
a swipe that scrolls the page over a chart shows nothing.

Keyboard: a chart with points is focusable. Left and Right move between
points, Home and End jump to the ends, and the tooltip follows the point.

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
  `CARD`, `GROUP`, `CONTROL`, `MENU_ITEM`, `NAV_ITEM` and `LIST_ITEM` (the
  shape of a selectable list item, shared by sidebar pages, places and file
  rows, with `LIST_EDGE` keeping the list's sides clear). It also holds the
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
  A slider with `gradient(stops)` draws its track in those colours (8 px tall) in place of the accent fill; the knob is unchanged.
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
  subpage slides in from the right over 320 ms, ease-out, over the page it was
  opened from. That page stays in place and dims slightly, and the new page is
  a card with a soft shadow on its leading edge, while the window's buttons
  stay at its top right. A pop slides the top page out to the right,
  revealing the page below, which undims. A second navigation during a slide
  starts from where the page is, with no snap. A sidebar switch cross-fades over
  120 ms. Motion is off under reduced motion (`tokens.motion(ms)` returns 0).
- Icons are the Bingux set under `packages/bingux-ui/icons/`; see
  [Icons](#icons) for how to use one and how to add another.
- A dropdown draws a symbolic icon before each option with `.icons(Vec<Icon>)`,
  in the same order as the options. Sound uses it for each device's kind.

## Icons

The icon set is the chat app's icon fork (`@chat/icons`, a rounded fork of
Phosphor: continuous "squircle" corners, its fill and weight choices and its own
custom icons). Icons are the fork's drawings, not redrawn: each SVG is the fork's
outline scaled to a 16 unit `viewBox` and tinted with the text colour. The Rust
names (`Icon::NetworkWired`) and the SVG file names (`network-wired-symbolic.svg`)
follow Adwaita's symbolic vocabulary, which the QML shell also uses.

```rust
use bingux_ui::icons::{Icon, icon};
use bingux_ui::style::{IconSize, Paint};

// A 16 px glyph in the muted text colour. Pass a token colour, never a literal.
icon(Icon::Search, IconSize::Control, Paint::Muted.color(&tokens))
```

| `IconSize`  | Regular / compact | Use                                         |
| ----------- | ----------------- | ------------------------------------------- |
| `Mark`      | 12 / 14 px        | Chevrons, ticks and marks inside a control  |
| `Text`      | 14 / 16 px        | A symbol in a line of text                  |
| `Control`   | 16 / 18 px        | Glyphs in buttons, fields and menu items    |
| `Row`       | 18 / 20 px        | Row and sidebar icons                       |
| `Hero`      | 48 px             | The icon of an empty or status page         |

### Weights and crispness

The fork has three weights. At 16 px, 16 units of its 256 unit grid are one pixel:
`regular` strokes are 1 px (too thin to carry an interface), `bold` strokes are
1.5 px, `fill` icons are solid masses with cut-outs. Bingux uses `bold` for outline
glyphs (frames, arrows, marks) and `fill` for solid masses (volume, bell, lock,
shield, discs, folders), the way chat uses its fill weight in buttons. Which
weight each icon uses is in `scripts/icons/labels.tsv`.

Crispness comes from the choice of weight and size, never from reshaping a curve.
`build.py` may move a whole icon by up to half a pixel so that its straight edges
land on pixel boundaries (`best_offset`); nothing else about the outline changes.
Draw icons at the sizes in the table (16 and 20 px are best) and let Skia
anti-alias the rest.

Icons that show a state (Wi-Fi signal, battery level, volume) are one drawing per
state with the same footprint, so the glyph does not jump when the state changes.

### Choose the icon for where it is used

Pick the conventional metaphor for the label the user reads next to the icon, not
the glyph whose name matches. A sidebar "Network" page needs an ethernet port, not
a node diagram; "Share" is a box with an up arrow, not a paper plane.
`scripts/icons/labels.tsv` records, for every icon, where it appears, the metaphor
people expect there, and the fork glyph or custom drawing that shows it.

### Add or change an icon

1. Find the fork glyph in `~/dev/chat/packages/icons/src/generated/<Name>.ts`.
2. Add a row to `scripts/icons/labels.tsv` (`file | where it appears | metaphor |
   <ForkName> <weight>`), then `scripts/icons/map-fork.py` writes
   `scripts/icons/src/<file>-symbolic.svg`:

   ```xml
   <svg viewBox="0 0 16 16">
     <fork name="SpeakerHigh" weight="fill" mode="exact"/>
   </svg>
   ```

   For an icon the fork does not have, mark the row `custom` and draw
   `src/<file>-symbolic.svg` by hand from plain SVG shapes on the 16 grid (see the
   header of `scripts/icons/build.py`: `stroke`, `fill`, `data-op="cut"`, `rx` for
   a continuous corner, and `<fork/>` pieces) in the fork's finish: round caps,
   round corners, the stroke width of its neighbours.
3. `scripts/icons/import-chat.py` vendors the fork geometry the sources name into
   `scripts/icons/fork/geometry.json`, so builds do not need a chat checkout.
4. Add the variant to `Icon`, to `Icon::ALL` and to `Icon::bytes()` in
   `packages/bingux-ui/src/icons.rs`.
5. `make icons` compiles the sources into `packages/bingux-ui/icons/` and the
   shell's copy; `make icons-check` fails when a source is invalid, a file is
   stale, or `icons.rs` names a missing file. Commit the sources and the output.
6. Look at it: `scripts/icons/sheets.py --out /tmp/icons` renders every icon at
   12, 14, 16, 18, 20 and 48 px, white on black and dark on light, beside the
   Adwaita original, with Freya's own rasteriser, grouped by where the icons are
   used (top bar, sidebar, controls, status, devices), plus a top-bar mock. Judge
   them at 1:1 and in the `@4x` copies.

The fork's geometry is MIT-licensed Phosphor work; `packages/bingux-ui/icons/COPYING`
carries the licence and attribution.

### In the QML shell

`scripts/icons/build.py` also writes `shell/bingux/icons/` (the same icons with
Adwaita's dark fill, which the shell's `SymbolicIcon` recolours) and
`shell/bingux/BinguxIconIndex.js` listing their names. The shell still resolves
icon names through `AdwaitaIcons.qml`, so nothing changes in the shell until it is
pointed at them: in `AdwaitaIcons.qml`, `import "BinguxIconIndex.js" as BinguxIcons`
and, after `canonical` is computed, return
`Qt.resolvedUrl("icons/" + canonical + ".svg")` when `BinguxIcons.names[canonical]`
is set. Names the shell asks for that are not in the set still fall through to the
Adwaita theme.

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

Every interactive control is a keyboard stop by default, and apps never opt in.
Buttons, icon buttons, switches, checkboxes, segments, dropdowns, sliders, text
fields, press rows, sidebar items, tabs, menu items, links and tree rows all take
focus themselves. A box an app builds from scratch (a swatch or a card) becomes a
stop through `focus_stop(content, label, role, enabled, radius, on_activate)` in
`controls/focus_stop.rs`, which gives it the same stop, ring, press and Enter/Space.
Non-interactive boxes are never stops.

Each control is exactly one Tab stop. Tab moves forward one stop and Shift+Tab
back one, in visual order, wrapping at the ends of the window. The window's header buttons are stops too, after the last page row and before the first, so Tab reaches them before it wraps. A composite control
(segmented, tab strip, radio group, tree, list) is one stop, and the arrow keys move
inside it (roving). A pointer click focuses without a ring, and the next Tab
continues from the clicked control.

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
window. F6 moves between focus regions (the sidebar, the content, the header), and
a region draws its ring around the whole area, not around one control. Region
sentinels are never Tab stops: Tab skips them without spending a press. Clicking a
control claims the press, so a pointer press never starts a ring. `trap()` wraps a
popup so Tab stays inside it while it is open, and closing it restores focus.
Use `use_focus_core()` to reach the core: with no host (the gallery, the snapshot
runner, an inspector window) it returns a detached core, so focus moves do nothing
and controls still work.
`step_focus(back)` moves focus to the next or previous stop as Tab does, with no
host needed. A list answers its Up and Down keys with it, so the arrows walk its
rows (the inspector's Accessibility list does).

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

## Inspector

Every app built with `bingux_ui::app::Application` includes a devtools window,
modelled on the Chrome and WebKit inspectors. Press Ctrl+Alt+Shift+I to open or
close it. It opens as a separate native window, and the app needs no code for
it.

The header is the shell's header bar: the inspect button (Ctrl+Shift+C) and
the phone preview toggle at the left, the Elements, Performance and
Accessibility tabs as a segmented rail in the middle (Ctrl+[ and Ctrl+] step
between them, Ctrl+1 to Ctrl+3 jump), and pause, refresh (F5) and the window
controls at the right. Under 760 px the tabs scroll and the tree and
Properties stack. Before the first snapshot the window says it is waiting for
an application window and shows a spinner; the first tree arrives as soon as
the app has laid out.

An app component can publish its call for the Properties pane:

```rust
rect()
    .component("Row")
    .published(|| {
        Published::call("row")
            .arg(Prop::text("title", title))
            .maybe_method(subtitle.map(|text| Prop::text("subtitle", text)))
    })
```

`Prop::text`, `flag`, `number` and `choice` say what kind of value a prop is,
which picks its editor; `.live()` plus `prop_override(a11y_id, "name")` in the
component lets the inspector change it while the app runs. `property_row` is
the dense row (name, hint, editor, reset) the pane is built from, and
`style::catalogue` lists every token by name for tools like this one.

- **Inspect mode** highlights the element under the pointer in the app with
  content, padding and margin bands and a card naming its tag, size, colour,
  font, padding, margin and accessible name, role and focusability. A click
  selects the element and reveals it in the tree; the click never reaches the
  app. Hold Shift to keep inspecting; Escape cancels.
- **Phone preview** forces the app window to the compact size class and touch
  pointer, so it shows the phone layout on a desktop; the inspector itself
  keeps its desktop density.
- **Elements** shows the live element tree as an outline, one line per element,
  indented by depth. A line is the element's name, then its text in quotes, then
  muted chips: size as `W×H`, layout and style values that differ from the
  defaults, `hidden` or `selected`, a `label` when the accessible name differs
  from the text, the accessibility role, and the element kind. The name is the
  bingux-ui component that built the element (`Row`, `Switch`, `PageHead`), else
  its accessibility role, else its layout role. A closed branch shows its child
  count. Click a triangle to open a branch, and Alt-click a triangle to open or
  shut the whole subtree under it. The right-click menu's Expand recursively
  does the same. Up and Down move the selection, Right opens or enters a
  branch, Left closes it or goes to the
  parent, Home and End jump to the ends. Ctrl+F searches names, text, labels and
  chips, with the hit count and Enter for the next hit. A right click offers
  Expand recursively, Collapse children, Copy text and Copy element; Copy element
  copies the outline format. The breadcrumb under the tree shows the selected
  element's ancestors by name; click one to select it. Hovering a row highlights
  its element in the app.
- **Properties**, beside the tree, shows the selected element in bingux-ui's
  own terms, not as CSS. Under it, three tabs: Properties, Layout and Computed.
  - *Component* shows the call that built a component that publishes one, such
    as `row("Wired connection 1")` or `Group::titled("VPN")`, with each
    argument as a field. Text arguments edit live; a prop edits live when the
    component opts in (the `Switch`'s `on` does), and is otherwise shown with
    "set in code".
  - The groups are named as the Rust API names them: *Layout* (`flex`, `items`,
    `justify`, `gap`, `clip`), *Spacing* (`p`, `px`, `py`, `m`, `mx`, `my`),
    *Size* (`w`, `h`, `min_w`, `max_w`, ...), *Paint* (`bg`, `rounded`,
    `shadow`, `border`, `opacity`), *Text style* (`font`, `ink`, `text`) and
    *Accessibility* (`name`, `focusable`, and the role).
  - Each row has the property's name, a hint line with the call it stands for
    and the value it resolves to (`gap(S3) = 12 px`), and an editor. Space,
    size, radius, colour, elevation, border, alignment and type style are
    dropdowns of the tokens, listing each token's value at the density the app
    window renders at (a phone preview shows the compact values); colours show
    a chip. A value on no token is a raw value: pick Raw value, type a number
    or `#rrggbb`, and the hint says "not a token". Numbers step with Up and
    Down. Enter or a pick applies the edit to the running app; an edited row
    shows its name in the accent and a reset button, and the header has Reset
    all.
  - Copy as Rust copies the element as bingux-ui code: the published call, or
    `rect().tw(&tokens, &[...])` with the utilities that are not defaults.
  - Layout draws the box as margin, padding and content bands, each side named
    by its space token, above the same spacing rows. Computed lists every
    resolved value, with colour chips, under a filter; every pane scrolls on
    its own.
- **Performance** records the app for a while: Record, use the app, Stop. The
  header shows the state (a dot and "Recording 3.2 s · 142 frames", then
  "Recorded …") and Clear drops the recording. Four stat tiles summarise the
  frames: frame rate while drawing, average frame, worst frame and the share
  over the 16.7 ms budget. Below, a `timeline_chart` lays every frame along a
  time axis with the budget as a dashed line and slow frames in red; hover a
  bar for its time and length. Drag across the timeline to select a stretch:
  the tiles and the call graph then describe only that stretch, and Clear (or a
  click on the chart, or Escape) returns to the whole recording. Call stacks
  sampled while recording are a `flame_graph` of the selected stretch: bars
  carry the function name, cut with an ellipsis, the detail line under the
  graph gives the full name, its share of all samples and of its own time, the
  bar under the pointer is highlighted, the application's own code is tinted in
  the accent colour, and a click zooms into a call (Escape or All calls zooms
  out). The graph starts past the calls at the top that only call the next one.
  Rust symbols are demangled, legacy and v0. Before the first recording the
  tab says what to do instead. Frames are timed and stacks sampled only while a
  recording runs; the view updates live. The tab scrolls when the window is too
  short for everything, and the tiles stack two by two when it is narrow.
  Keyboard: Tab reaches Record, the timeline and the call graph. On the
  timeline Left and Right move between frames, Shift with an arrow selects, and
  Escape clears. On the graph Down, Up, Left and Right walk callee, caller and
  siblings, Enter zooms in and Escape zooms out.
- **Accessibility** lists what the checks found in the app. The top shows
  stat tiles with the counts of errors, warnings and notes, and a segmented
  filter by severity beside Collapse all and Expand all. Issues sit in one card
  per kind (Naming, Focus, Target size, Contrast, Geometry, Layout shift,
  Motion, Performance). The card's header names the kind with its count, says
  why such problems matter, and folds the issues beneath it. An issue row has a
  severity icon, what is wrong, the element (`Button #485 "Save"`: the component
  that built it, its id and its name or text) and how to fix it. Hovering an
  issue outlines its element in the app. Pressing it, or Enter on it, selects
  the element in Elements, opens the tree down to it and scrolls it into view,
  and leaves it outlined in the app. Up and Down move between rows, and Tab and
  Shift+Tab move between every control. Before the first snapshot the tab says
  it is waiting for an application window, and a window that passes every check
  shows an empty state.

Environment variables for tests and screenshots:

- `BINGUX_INSPECT_OPEN=1` opens the inspector with the first application
  window.
- `BINGUX_INSPECT_SHOT=<dir>` writes `inspector.png` and `app.png` to `<dir>`,
  at most once every 1.5 seconds. Without it, nothing is read back from the
  canvas.
