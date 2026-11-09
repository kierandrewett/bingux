//! The window: a header bar with the mode, the display and keys of that
//! mode, and the history beside them (a screen of its own on a narrow window).

use bingux_calc::store::Mode;
use bingux_ui::prelude::*;
use bingux_ui::shell::MenuItem;
use bingux_ui::theme::Tokens;
use freya::prelude::*;

use crate::display::Display;
use crate::history::HistoryPanel;
use crate::model::Calc;
use crate::modes::{ConvertPanel, ProgrammerPanel};
use crate::pads;

/// What the shortcuts dialog says.
const SHORTCUTS: &str = "\
Enter  Finish the calculation\n\
Escape  Clear the entry, then everything\n\
Backspace  Delete the last character\n\
Ctrl+Shift+C  Copy the result\n\
Ctrl+H  Show or hide the history\n\
Ctrl+1 to 4  Basic, Scientific, Programmer, Convert\n\
+ - * / x ^  Operators; x and * both multiply\n\
%  Percent: 200 + 10% is 220\n\
Ctrl+Shift+?  About Calculator";

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

        let options: Vec<(String, String)> = Mode::ALL
            .iter()
            .map(|m| (mode_key(*m), m.name().to_string()))
            .collect();
        let pick = calc.clone();
        let switch = segmented(options, &mode_key(mode), move |key: String| {
            pick.set_mode(mode_of(&key))
        });

        let toggle = calc.clone();
        let showing_history = if compact { history_page } else { history_open };
        let history_button = icon_button(Icon::Clock, move |_: ()| toggle_history(&toggle))
            .tooltip(if showing_history {
                "Hide history (Ctrl+H)"
            } else {
                "Show history (Ctrl+H)"
            })
            .active(showing_history);

        let header = HeaderBar::new("Calculator")
            .title_slot(switch)
            .leading(history_button)
            .trailing(menu_button("Main menu", main_menu(&calc, overlays)).icon(Icon::OpenMenu))
            .trailing(WindowControls);

        let key_calc = calc.clone();
        let copy_calc = calc.clone();
        let body = if compact && history_page {
            rect()
                .tw(&tokens, &[w_full(), h_flex()])
                .child(HistoryPanel { calc: calc.clone() })
                .into_element()
        } else {
            let main = calculator(&calc, mode, &tokens);
            let mut row_el = rect()
                .tw(&tokens, &[w_full(), h_flex(), flex_row(), grow_children()])
                .child(main);
            if !compact && history_open {
                row_el = row_el.child(
                    rect()
                        .tw(&tokens, &[w(Dim::Sidebar), h_full(), bg(Paint::Sidebar)])
                        .child(HistoryPanel { calc: calc.clone() }),
                );
            }
            row_el.into_element()
        };

        rect()
            .tw(&tokens, &[w_full(), h_full(), flex_col(), bg(Paint::Bg)])
            .on_key_down(move |event: Event<KeyboardEventData>| {
                if shortcut(&key_calc, overlays, &event) {
                    event.stop_propagation();
                }
            })
            .on_global_key_down(move |event: Event<KeyboardEventData>| {
                let chord = event.modifiers.intersects(Modifiers::ctrl_or_meta())
                    && event.modifiers.contains(Modifiers::SHIFT);
                if chord && matches!(&event.key, Key::Character(c) if c.eq_ignore_ascii_case("c")) {
                    let shown = copy_calc.shown();
                    copy_calc.copy_text(shown.copy, overlays);
                }
            })
            .child(header)
            .child(body)
    }
}

/// Opens or closes the history: a setting on a desktop, a screen on a phone.
fn toggle_history(calc: &Calc) {
    if is_compact() {
        let mut page = calc.history_page;
        let next = !*page.peek();
        page.set(next);
    } else {
        calc.with(|s| s.saved.settings.history_open = !s.saved.settings.history_open);
        calc.changed();
    }
}

/// Keys the window takes when no control did: Enter, Escape, Backspace and
/// any character typed while a key of the keypad (not the field) has focus.
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
        Key::Named(NamedKey::Escape) => {
            calc.clear();
            true
        }
        Key::Named(NamedKey::Backspace) if !ctrl && !alt => {
            calc.backspace();
            true
        }
        Key::Character(text) if ctrl && !alt && text.eq_ignore_ascii_case("h") => {
            toggle_history(calc);
            true
        }
        Key::Character(text) if ctrl && !alt && matches!(text.as_str(), "1" | "2" | "3" | "4") => {
            let index = text.parse::<usize>().unwrap_or(1) - 1;
            calc.set_mode(Mode::ALL[index]);
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
            // Programmer has more than a window's height of controls, so it
            // scrolls, and its keypads have a height of their own.
            let rows = 5.0;
            let height = Dim::Button.px() * rows + S2.px() * (rows - 1.0);
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
            let inner = rect()
                .tw(tokens, &[w_full(), flex_col(), gap(S3), p(S4)])
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

fn main_menu(calc: &Calc, overlays: Overlays) -> Vec<MenuItem> {
    let (group, precision) = calc.with(|s| (s.saved.settings.group, s.saved.settings.precision));
    let group_calc = calc.clone();
    let mut items = vec![
        menu_item("Group digits", move |_| {
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
            menu_item(format!("{digits} digits"), move |_| {
                calc.with(|s| s.set_precision(digits));
                calc.changed();
            })
            .checked(precision == digits)
        })
        .collect();
    items.push(submenu("Precision", precisions));
    items.push(menu_separator());
    let history_calc = calc.clone();
    items.push(menu_item("Clear history", move |_| history_calc.clear_history()).destructive());
    items.push(menu_separator());
    items.push(menu_item("Keyboard shortcuts", move |_| {
        overlays.show_dialog(dialog("Keyboard shortcuts", SHORTCUTS).cancel("Close"));
    }));
    items.extend(bingux_ui::about::app_menu_items(overlays));
    items
}
