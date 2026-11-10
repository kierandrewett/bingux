//! The window: a header bar with the mode switcher, the display and keys of
//! that mode, and the history beside them (a screen of its own on a narrow
//! window).

use std::rc::Rc;
use std::time::Duration;

use bingux_calc::store::Mode;
use bingux_ui::nav::{RouteTable, use_try_location};
use bingux_ui::prelude::*;
use bingux_ui::shell::MenuItem;
use bingux_ui::theme::Tokens;
use freya::prelude::*;

use crate::display::Display;
use crate::history::HistoryPanel;
use crate::model::Calc;
use crate::modes::{ConvertPanel, ProgrammerPanel};
use crate::pads;

/// The window root. The content is below the overlay host, which provides
/// the window handle and the menus.
#[derive(PartialEq)]
pub struct App;

impl Component for App {
    fn render(&self) -> impl IntoElement {
        let tokens = provide_tokens();
        rect()
            .tw(&tokens, &[w_full(), h_full(), ink(Paint::Text)])
            .font_family(bingux_ui::theme::font_family())
            .child(OverlayHost::new(Content))
    }
}

#[derive(PartialEq)]
struct Content;

/// The address a mode has: `bingux-calculator://scientific`. A launch with a
/// mode (`bingux-calculator --mode convert`, or the desktop entry's actions)
/// opens it, in a new window or the one that is open.
fn slug(mode: Mode) -> String {
    mode.name().to_lowercase()
}

/// The modes as places, so a second launch reaches the open window.
pub fn routes() -> RouteTable {
    // The table keeps its titles for the life of the process; the window
    // shows the app's title from the catalogue, which follows the language.
    let title: &'static str = Box::leak(tr!("calculator-title").into_boxed_str());
    let mut table = RouteTable::new("bingux-calculator")
        .mode(bingux_ui::nav::Mode::Linear)
        .index(title, |_| rect().into());
    for mode in Mode::ALL {
        table = table.root(&slug(mode)).title(title);
    }
    table
}

/// The mode a location names, if it names one.
fn mode_at(location: &Location) -> Option<Mode> {
    let first = location.segments().first()?;
    Mode::ALL
        .iter()
        .copied()
        .find(|mode| slug(*mode) == first.to_lowercase())
}

/// Shows `mode` and records it as the window's place, so the same address
/// opened again (from the desktop entry) is a change and not a no-op.
fn switch_mode(calc: &Calc, mode: Mode) {
    calc.set_mode(mode);
    bingux_ui::nav::navigate(&slug(mode));
}

/// The name of a mode as shown, in the language shown. `Mode::name` stays
/// the English key of addresses and the saved state.
pub fn mode_title(mode: Mode) -> String {
    match mode {
        Mode::Basic => tr!("calculator-mode-basic"),
        Mode::Scientific => tr!("calculator-mode-scientific"),
        Mode::Programmer => tr!("calculator-mode-programmer"),
        Mode::Convert => tr!("calculator-mode-convert"),
    }
}

/// What failed, in the language shown. The parser's detail of a syntax error,
/// a domain error and a unit error comes from `bingux_calc` in English.
pub fn error_text(error: &bingux_calc::CalcError) -> String {
    use bingux_calc::CalcError;
    match error {
        CalcError::DivideByZero => tr!("calculator-divide-by-zero-error"),
        CalcError::NotReal => tr!("calculator-not-real-error"),
        CalcError::Undefined(what) => tr!("calculator-undefined-error", function = *what),
        CalcError::Incomplete => tr!("calculator-incomplete-error"),
        CalcError::TooLarge => tr!("calculator-too-large-error"),
        CalcError::Unknown(name) => tr!("calculator-unknown-name-error", name = name),
        CalcError::NeedsInteger(what) => tr!("calculator-needs-integer-error", function = *what),
        CalcError::NoRate(code) => tr!("calculator-no-rate-error", currency = code),
        CalcError::Syntax { .. } | CalcError::Domain(_) | CalcError::Unit(_) => error.to_string(),
    }
}

fn mode_key(mode: Mode) -> String {
    mode.name().to_string()
}

fn mode_of(key: &str) -> Mode {
    Mode::ALL
        .iter()
        .copied()
        .find(|m| m.name() == key)
        .unwrap_or(Mode::Basic)
}

impl Component for Content {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let overlays = use_overlays();
        let calc = Calc::use_new();
        let mode = calc.mode();
        let compact = is_compact();
        let history_page = *calc.history_page.read();
        let history_open = calc.with(|s| s.saved.settings.history_open);

        let pick = calc.clone();
        // The mode decides which calculator is on screen, so it is chrome: the
        // header's switcher on a desktop, the bottom bar on a phone.
        let modes = view_switcher(
            Mode::ALL
                .iter()
                .map(|m| view(mode_key(*m), mode_title(*m)).icon(mode_icon(*m))),
            &mode_key(mode),
            move |key: String| switch_mode(&pick, mode_of(&key)),
        );

        // A launch that names a mode (`--mode convert`) puts it here.
        let wanted = use_try_location().as_ref().and_then(mode_at);
        let routed = calc.clone();
        use_side_effect_with_deps(&wanted, move |wanted| {
            if let Some(mode) = wanted {
                routed.set_mode(*mode);
            }
        });

        // The window remembers its size once a resize has stood still.
        let size = bingux_ui::surface::window_size();
        let size = (size.width.round() as u32, size.height.round() as u32);
        let remember = calc.clone();
        let mut pending = use_state::<Option<TaskHandle>>(|| None);
        use_side_effect_with_deps(&size, move |&(width, height)| {
            if remember.with(|s| s.saved.settings.window) == Some((width, height))
                || width < 200
                || height < 200
            {
                return;
            }
            if let Some(task) = pending.take() {
                task.cancel();
            }
            let remember = remember.clone();
            pending.set(Some(spawn(async move {
                timer(Duration::from_millis(600)).await;
                remember.with(|s| s.saved.settings.window = Some((width, height)));
                remember.save();
            })));
        });

        // The history is the aside: a pane beside the keys on a desktop, a
        // page pushed over them on a phone (Back, or using an entry, which
        // clears `history_page`, returns to the keys).
        let open_history: Rc<dyn Fn()> = {
            let calc = calc.clone();
            Rc::new(move || {
                if is_compact() {
                    let mut page = calc.history_page;
                    let next = !*page.peek();
                    page.set(next);
                } else {
                    calc.with(|s| s.saved.settings.history_open = !s.saved.settings.history_open);
                    calc.changed();
                }
            })
        };
        let showing_history = if compact { history_page } else { history_open };
        // The window's commands: the palette, the shortcuts sheet and the
        // main menu list them with their keys.
        use_window_commands(window_commands(
            &calc,
            overlays,
            showing_history,
            open_history.clone(),
        ));
        let history_button = {
            let open_history = open_history.clone();
            icon_button(Icon::Clock, move |_: ()| open_history()).tooltip(&if showing_history {
                tr!("calculator-history-hide.tooltip")
            } else {
                tr!("calculator-history-show.tooltip")
            })
        };

        let menu = app_menu(main_menu(&calc));
        let header = HeaderBar::new(tr!("calculator-title")).switcher(modes);
        let header = if compact && history_page {
            let back = calc.clone();
            HeaderBar::new(tr!("calculator-history-title")).leading(
                icon_button(Icon::GoPrevious, move |_| {
                    let mut page = back.history_page;
                    page.set(false);
                })
                .tooltip(&tr!("calculator-history-hide.tooltip")),
            )
        } else {
            header.trailing(history_button)
        };
        let header = header.trailing(menu).trailing(WindowControls);

        let key_calc = calc.clone();
        let mut screen = Screen::new(header).content(calculator(&calc, mode, &tokens));
        if (!compact && history_open) || (compact && history_page) {
            screen = screen.aside(HistoryPanel { calc: calc.clone() });
        }
        if compact && history_page {
            screen = screen.compact_pane(Pane::Aside);
        }

        rect()
            .tw(&tokens, &[w_full(), h_full()])
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if shortcut(&key_calc, overlays, &event) {
                    event.stop_propagation();
                }
            })
            .child(screen)
    }
}

/// The icon of a mode, for a phone's bottom bar.
fn mode_icon(mode: Mode) -> Icon {
    match mode {
        Mode::Basic => Icon::AccessoriesCalculator,
        Mode::Scientific => Icon::ViewGrid,
        Mode::Programmer => Icon::FormatCode,
        Mode::Convert => Icon::Swap,
    }
}

/// The window's commands: the modes, copy, clear and history. The palette,
/// the shortcuts sheet and the main menu list them, with their keys.
fn window_commands(
    calc: &Calc,
    overlays: Overlays,
    showing_history: bool,
    open_history: Rc<dyn Fn()>,
) -> Vec<Command> {
    let keys = ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4"];
    let mut commands: Vec<Command> = Mode::ALL
        .iter()
        .enumerate()
        .map(|(index, &mode)| {
            let calc = calc.clone();
            let checked = calc.mode() == mode;
            Command::new(
                format!("calculator.mode.{}", slug(mode)),
                mode_title(mode),
                move |_| switch_mode(&calc, mode),
            )
            .shortcut(keys[index])
            .checked(checked)
            .group(CommandGroup::View)
        })
        .collect();
    let copy = calc.clone();
    commands.push(
        Command::new(
            "calculator.copy",
            tr!("calculator-command-copy"),
            move |_| {
                let shown = copy.shown();
                copy.copy_text(shown.copy, overlays);
            },
        )
        .shortcut("Ctrl+Shift+C")
        .group(CommandGroup::Edit),
    );
    let clear = calc.clone();
    commands.push(
        Command::new(
            "calculator.clear",
            tr!("calculator-command-clear"),
            move |_| clear.clear(),
        )
        .shortcut("Escape")
        .group(CommandGroup::Edit),
    );
    commands.push(
        Command::new(
            "calculator.history",
            tr!("calculator-command-history"),
            move |_| open_history(),
        )
        .shortcut("Ctrl+H")
        .checked(showing_history)
        .group(CommandGroup::View),
    );
    commands
}

/// Keys the window takes when no control did: Enter, Backspace and any
/// character typed while a key of the keypad (not the field) has focus.
fn shortcut(calc: &Calc, overlays: Overlays, event: &Event<KeyboardEventData>) -> bool {
    if overlays.dialog_open() {
        return false;
    }
    let ctrl = event.modifiers.intersects(Modifiers::ctrl_or_meta());
    let alt = event.modifiers.contains(Modifiers::ALT);
    match &event.key {
        Key::Named(NamedKey::Enter) if !ctrl && !alt => {
            calc.equals();
            true
        }
        Key::Named(NamedKey::Backspace) if !ctrl && !alt => {
            calc.backspace();
            true
        }
        Key::Character(text) if !ctrl && !alt && text.chars().count() == 1 && text != " " => {
            calc.type_text(text);
            true
        }
        _ => false,
    }
}

/// The display and keys of one mode.
fn calculator(calc: &Calc, mode: Mode, tokens: &Tokens) -> Element {
    let display = Display { calc: calc.clone() };
    let id = calc.field_id;
    let compact = is_compact();
    let mut column = rect()
        .tw(
            tokens,
            &[
                w_full(),
                h_flex(),
                flex_col(),
                gap(S3),
                p(S4),
                grow_children(),
            ],
        )
        .child(display);
    let widest = match mode {
        Mode::Basic | Mode::Convert => Dim::PagePhone,
        _ => Dim::Page,
    };
    match mode {
        Mode::Basic => {
            column = column.child(pad_slot(keypad(4, pads::basic_keys(calc)).refocus(id)));
        }
        Mode::Scientific => {
            let functions = keypad(6, pads::scientific_keys(calc)).refocus(id);
            let numbers = keypad(4, pads::basic_keys(calc)).refocus(id);
            column = column.child(paired(tokens, compact, functions, numbers, 6.0, 4.0));
        }
        Mode::Programmer => {
            // Programmer has more than a phone's height of controls, so it
            // scrolls, and its keypads have a height of their own.
            let rows = 5.0;
            let key = if compact { Dim::Button } else { Dim::Control };
            let height = key.px() * rows + S2.px() * (rows - 1.0);
            let functions = keypad(4, pads::programmer_function_keys(calc)).refocus(id);
            let numbers = keypad(4, pads::programmer_keys(calc)).refocus(id);
            let pads = if compact {
                rect()
                    .width(Size::fill())
                    .tw(tokens, &[flex_col(), gap(S2)])
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(height))
                            .child(functions),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .height(Size::px(height))
                            .child(numbers),
                    )
            } else {
                rect()
                    .width(Size::fill())
                    .height(Size::px(height))
                    .tw(tokens, &[flex_row(), gap(S3), grow_children()])
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::fill())
                            .child(functions),
                    )
                    .child(
                        rect()
                            .width(Size::flex(1.0))
                            .height(Size::fill())
                            .child(numbers),
                    )
            };
            // The window's common size shows all of it: a tighter rhythm than
            // the other modes, and it scrolls only below that.
            let inner = rect()
                .tw(
                    tokens,
                    &[
                        w_full(),
                        flex_col(),
                        gap(if compact { S3 } else { S2 }),
                        p(if compact { S4 } else { S3 }),
                    ],
                )
                .child(Display { calc: calc.clone() })
                .child(ProgrammerPanel { calc: calc.clone() })
                .child(pads);
            return clamp(
                widest,
                rect()
                    .tw(tokens, &[w_full(), h_flex()])
                    .child(ScrollView::new().child(inner)),
            )
            .into_element();
        }
        Mode::Convert => {
            column = column.child(ConvertPanel { calc: calc.clone() });
            column = column.child(pad_slot(keypad(4, pads::convert_keys(calc)).refocus(id)));
        }
    }
    clamp(widest, column).into_element()
}

/// A keypad that takes the room that is left.
fn pad_slot(pad: Keypad) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::flex(1.0))
        .child(pad)
}

/// Two keypads side by side, or one above the other in a narrow window.
fn paired(
    tokens: &Tokens,
    compact: bool,
    first: Keypad,
    second: Keypad,
    first_weight: f32,
    second_weight: f32,
) -> Rect {
    if compact {
        rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .tw(tokens, &[flex_col(), gap(S2), grow_children()])
            .child(pad_slot(first))
            .child(pad_slot(second))
    } else {
        rect()
            .width(Size::fill())
            .height(Size::flex(1.0))
            .tw(tokens, &[flex_row(), gap(S3), grow_children()])
            .child(
                rect()
                    .width(Size::flex(first_weight))
                    .height(Size::fill())
                    .child(first),
            )
            .child(
                rect()
                    .width(Size::flex(second_weight))
                    .height(Size::fill())
                    .child(second),
            )
    }
}

fn main_menu(calc: &Calc) -> Vec<MenuItem> {
    let (group, precision) = calc.with(|s| (s.saved.settings.group, s.saved.settings.precision));
    let group_calc = calc.clone();
    let mut items = vec![
        menu_item(tr!("calculator-menu-group-digits"), move |_| {
            group_calc.with(|s| s.set_group(!s.saved.settings.group));
            group_calc.changed();
        })
        .checked(group),
    ];
    let precisions = [10usize, 16, 24, 32, 50]
        .iter()
        .map(|digits| {
            let calc = calc.clone();
            let digits = *digits;
            menu_item(tr!("calculator-menu-digits", count = digits), move |_| {
                calc.with(|s| s.set_precision(digits));
                calc.changed();
            })
            .checked(precision == digits)
        })
        .collect();
    items.push(submenu(tr!("calculator-menu-precision"), precisions));
    items.push(menu_separator());
    let history_calc = calc.clone();
    items.push(
        menu_item(tr!("calculator-history-clear-menu"), move |_| {
            history_calc.clear_history()
        })
        .destructive(),
    );
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> Option<Mode> {
        Location::parse_in("bingux-calculator", text)
            .as_ref()
            .and_then(mode_at)
    }

    #[test]
    fn a_launch_names_a_mode_by_its_address() {
        assert_eq!(at("convert"), Some(Mode::Convert));
        assert_eq!(at("bingux-calculator://programmer"), Some(Mode::Programmer));
        assert_eq!(at("Scientific"), Some(Mode::Scientific));
    }

    #[test]
    fn an_unknown_or_missing_mode_changes_nothing() {
        assert_eq!(at("graphing"), None);
        assert_eq!(at(""), None);
    }

    #[test]
    fn every_mode_has_a_route_and_the_route_names_it() {
        let table = routes();
        for mode in Mode::ALL {
            let location =
                Location::parse_in("bingux-calculator", &slug(mode)).expect("an address");
            assert!(table.resolve(&location).is_some(), "{mode:?} has a route");
            assert_eq!(mode_at(&location), Some(mode));
        }
    }
}
