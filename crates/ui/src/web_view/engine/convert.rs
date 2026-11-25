//! Field for field: the host's words for input and the engine's.
//!
//! [`PageInput`] already uses the engine's own names and bit values, so each
//! conversion copies fields and loses none. The tests are the proof that every
//! field arrives.

use vivarium::diagnostics::ProcessReason;
use vivarium::environment::{ColorScheme, Environment};
use vivarium::error::{Error, Result, Status};
use vivarium::input::{
    self as vi, Button, Buttons, ContextMenuOrigin, CursorKind, FocusMove, KeyAction, KeyLocation,
    Modifiers, Phase, TouchKind as EngineTouch, WheelFlags, WheelMode,
};
use vivarium::security::{SecurityFlags, SecurityState as EngineSecurity};
use vivarium::session::{Session, WindowState};

use crate::frame_view::{CursorKind as ViewCursor, WheelPhase};
use crate::web_view::input::{
    EditCommand, FocusAction, KeyEvent, PageInput, PointerEvent, PointerKind, TextEvent,
    TouchEvent, TouchKind, WheelEvent, WheelUnit,
};
use crate::web_view::page::{GoneReason, Security, SecurityState};

fn modifiers(bits: u32) -> Modifiers {
    Modifiers::from_bits_truncate(bits)
}

/// The engine's clock, with which input is stamped: what the page sees as the
/// event's `timeStamp` is when the person acted, not when the engine took it
/// from the queue.
fn now() -> u64 {
    #[cfg(target_os = "linux")]
    {
        vivarium::engine::now_ns()
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

pub(super) fn pointer(event: &PointerEvent) -> vi::PointerEvent {
    let kind = match event.kind {
        PointerKind::Move => vi::PointerKind::Move,
        PointerKind::Down => vi::PointerKind::Down,
        PointerKind::Up => vi::PointerKind::Up,
        PointerKind::Enter => vi::PointerKind::Enter,
        PointerKind::Leave => vi::PointerKind::Leave,
    };
    let mut out = vi::PointerEvent::mouse(kind, event.x, event.y)
        .with_button(Button::from_raw(event.button))
        .with_buttons(Buttons::from_bits_truncate(event.buttons))
        .with_modifiers(modifiers(event.modifiers.0))
        .with_click_count(event.click_count);
    out.time_ns = now();
    out
}

/// The text colour for something drawn in `accent` (`0xAARRGGBB`): white as long
/// as it keeps a contrast of 3 to 1 with it (the least WCAG asks of an
/// interface component, and what the shell's blue and every Adwaita accent but
/// the yellow ones are drawn with), else black.
pub(super) fn on_accent(accent: u32) -> u32 {
    let channel = |shift: u32| {
        let value = f64::from((accent >> shift) & 0xFF) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    // Contrast with white is 1.05 / (luminance + 0.05): at least 3 below 0.30.
    if luminance <= 0.30 {
        0xFFFF_FFFF
    } else {
        0xFF00_0000
    }
}

/// The engine-wide environment for the host's appearance. ABI 2.0 takes the
/// colour scheme, the accent and a palette of system colours; the host's
/// motion, contrast, text scale and locales have no field there, so pages
/// keep the engine's own for those.
pub(super) fn environment(
    environment: &crate::web_view::environment::PageEnvironment,
) -> Environment {
    Environment {
        color_scheme: Some(if environment.dark() {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }),
        accent: Some((environment.accent, on_accent(environment.accent))),
        palette: None,
    }
}

/// What a page sees for one notch of a wheel: three lines, as Firefox on GTK
/// reports (`deltaY` 3 in `DOM_DELTA_LINE`, which scrolls about 57 px at the
/// default font). The host counts notches (a hi-res wheel's fraction stays);
/// the engine is told lines, so a notch scrolls what it scrolls in Firefox.
const LINES_PER_NOTCH: f64 = 3.0;

pub(super) fn wheel(event: &WheelEvent) -> vi::WheelEvent {
    let mut out = vi::WheelEvent::pixels(event.x, event.y, event.dx, event.dy)
        .with_modifiers(modifiers(event.modifiers.0));
    match event.unit {
        WheelUnit::Pixel => {}
        WheelUnit::Line => {
            out.mode = WheelMode::Line;
            out.dx = event.dx * LINES_PER_NOTCH;
            out.dy = event.dy * LINES_PER_NOTCH;
            out.ticks = Some((event.dx, event.dy));
            out.lines = Some((
                (event.dx * LINES_PER_NOTCH).round() as i32,
                (event.dy * LINES_PER_NOTCH).round() as i32,
            ));
        }
    }
    out.phase = match event.phase {
        WheelPhase::None => Phase::None,
        WheelPhase::Began => Phase::Began,
        WheelPhase::Changed => Phase::Changed,
        WheelPhase::Ended => Phase::Ended,
        WheelPhase::Cancelled => Phase::Canceled,
    };
    if event.precise {
        out.flags |= WheelFlags::PRECISE;
    }
    out.time_ns = now();
    out
}

pub(super) fn key(event: &KeyEvent) -> vi::KeyEvent {
    let action = if event.down {
        KeyAction::Down
    } else {
        KeyAction::Up
    };
    let mut out = vi::KeyEvent::new(action, event.key.clone(), event.code.clone())
        .with_modifiers(modifiers(event.modifiers.0))
        .with_text(event.text.clone());
    out.repeat = event.repeat;
    out.location = KeyLocation::from_raw(event.location);
    out.time_ns = now();
    out
}

pub(super) fn touch(event: &TouchEvent) -> vi::TouchEvent {
    let kind = match event.kind {
        TouchKind::Down => EngineTouch::Down,
        TouchKind::Move => EngineTouch::Move,
        TouchKind::Up => EngineTouch::Up,
        TouchKind::Cancel => EngineTouch::Cancel,
    };
    let mut out = vi::TouchEvent::finger(kind, event.id as u32, event.x, event.y);
    out.time_ns = now();
    out
}

pub(super) fn edit(command: EditCommand) -> vi::EditCommand {
    match command {
        EditCommand::Cut => vi::EditCommand::Cut,
        EditCommand::Copy => vi::EditCommand::Copy,
        EditCommand::Paste => vi::EditCommand::Paste,
        EditCommand::PastePlain => vi::EditCommand::PastePlain,
        EditCommand::SelectAll => vi::EditCommand::SelectAll,
        EditCommand::Undo => vi::EditCommand::Undo,
        EditCommand::Redo => vi::EditCommand::Redo,
        EditCommand::Delete => vi::EditCommand::Delete,
    }
}

/// A focus action is one of two calls: whether the window has the keyboard
/// at all is its [`WindowState`], where the focus goes inside the page is a
/// [`FocusMove`].
pub(super) enum Focus {
    Window(WindowState),
    Move(FocusMove),
}

pub(super) fn focus(action: FocusAction) -> Focus {
    match action {
        // A page whose window takes the keyboard is shown; one that loses it stays shown.
        FocusAction::Activate => Focus::Window(WindowState::Active),
        FocusAction::Deactivate => Focus::Window(WindowState::Visible),
        FocusAction::Next => Focus::Move(FocusMove::Next),
        FocusAction::Previous => Focus::Move(FocusMove::Previous),
        FocusAction::First => Focus::Move(FocusMove::First),
        FocusAction::Last => Focus::Move(FocusMove::Last),
    }
}

/// What the person can do with a tab's window: hidden, shown, or shown with
/// the keyboard.
pub(super) fn window_state(visible: bool, focused: bool) -> WindowState {
    match (visible, focused) {
        (false, _) => WindowState::Hidden,
        (true, false) => WindowState::Visible,
        (true, true) => WindowState::Active,
    }
}

pub(super) fn context_menu(
    at: Option<(f64, f64)>,
    held: crate::web_view::input::Mods,
) -> vi::ContextMenuEvent {
    match at {
        Some((x, y)) => {
            let mut event = vi::ContextMenuEvent::at(x, y);
            event.modifiers = modifiers(held.0);
            event
        }
        None => vi::ContextMenuEvent {
            origin: ContextMenuOrigin::Keyboard,
            modifiers: modifiers(held.0),
            x: 0.0,
            y: 0.0,
        },
    }
}

/// Send `input` to `session`, the one call it is. ABI 2.0 has no
/// composition call: a preedit is `Unsupported` (the capability says so, and
/// the input method commits text instead).
pub(super) fn send(session: &Session, input: &PageInput) -> Result<()> {
    match input {
        PageInput::Pointer(event) => session.send_pointer(&pointer(event)),
        PageInput::Wheel(event) => session.send_wheel(&wheel(event)),
        PageInput::Key(event) => session.send_key(&key(event)),
        PageInput::Text(TextEvent::Commit(text)) => session.send_text(text),
        PageInput::Text(TextEvent::Preedit { .. }) => Err(Error::from(Status::Unsupported)),
        PageInput::Touch(event) => session.send_touch(&touch(event)),
        PageInput::Edit(command) => session.edit(edit(*command)),
        PageInput::Focus(action) => match focus(*action) {
            Focus::Window(state) => session.set_window_state(state),
            Focus::Move(to) => session.focus(to),
        },
        PageInput::ContextMenu { at, modifiers } => {
            session.send_context_menu(&context_menu(*at, *modifiers))
        }
    }
}

pub(super) fn security(state: EngineSecurity, flags: SecurityFlags) -> Security {
    Security {
        state: match state {
            EngineSecurity::Secure => SecurityState::Secure,
            EngineSecurity::Broken => SecurityState::Broken,
            EngineSecurity::Local => SecurityState::Local,
            _ => SecurityState::Insecure,
        },
        mixed_active: flags.contains(SecurityFlags::MIXED_ACTIVE),
        mixed_passive: flags.contains(SecurityFlags::MIXED_DISPLAY),
        extended_validation: flags.contains(SecurityFlags::EV),
    }
}

pub(super) fn gone(reason: ProcessReason) -> GoneReason {
    match reason {
        ProcessReason::HungKilled => GoneReason::Killed,
        // A process that ran out of memory crashed, as far as the tab is concerned.
        _ => GoneReason::Crashed,
    }
}

/// The pointer shape of the engine as the view draws it. A cursor that is an
/// image has no name to draw it by, so it is the arrow.
pub(super) fn cursor(kind: CursorKind) -> ViewCursor {
    match kind {
        CursorKind::Pointer => ViewCursor::Pointer,
        CursorKind::Text => ViewCursor::Text,
        CursorKind::Wait => ViewCursor::Wait,
        CursorKind::Progress => ViewCursor::Progress,
        CursorKind::Help => ViewCursor::Help,
        CursorKind::Crosshair => ViewCursor::Crosshair,
        CursorKind::Move => ViewCursor::Move,
        CursorKind::Grab => ViewCursor::Grab,
        CursorKind::Grabbing => ViewCursor::Grabbing,
        CursorKind::NotAllowed => ViewCursor::NotAllowed,
        CursorKind::ResizeNs => ViewCursor::NsResize,
        CursorKind::ResizeEw => ViewCursor::EwResize,
        CursorKind::ResizeNesw => ViewCursor::NeswResize,
        CursorKind::ResizeNwse => ViewCursor::NwseResize,
        CursorKind::ZoomIn => ViewCursor::ZoomIn,
        CursorKind::ZoomOut => ViewCursor::ZoomOut,
        CursorKind::None => ViewCursor::Hidden,
        _ => ViewCursor::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web_view::environment::{PageEnvironment, PageScheme};
    use crate::web_view::input::Mods;

    #[test]
    fn the_hosts_appearance_becomes_the_engines_environment() {
        let host = PageEnvironment {
            scheme: PageScheme::Dark,
            accent: 0xFF11_2233,
            reduced_motion: true,
            more_contrast: false,
            text_scale: 1.5,
            locales: vec!["de-DE".to_string()],
        };
        let engine = environment(&host);
        assert_eq!(engine.color_scheme, Some(ColorScheme::Dark));
        assert_eq!(
            engine.accent,
            Some((0xFF11_2233, 0xFFFF_FFFF)),
            "white on a dark accent"
        );
        assert_eq!(
            engine.palette, None,
            "the engine keeps its own system colours"
        );
        assert_eq!(
            environment(&PageEnvironment::default()).color_scheme,
            Some(ColorScheme::Light)
        );
    }

    #[test]
    fn the_text_on_the_accent_is_the_one_that_can_be_read() {
        assert_eq!(
            on_accent(0xFF35_84E4),
            0xFFFF_FFFF,
            "the shell's blue takes white"
        );
        assert_eq!(on_accent(0xFF26_A269), 0xFFFF_FFFF, "a green takes white");
        assert_eq!(on_accent(0xFFF6_D32D), 0xFF00_0000, "a yellow takes black");
        assert_eq!(on_accent(0xFF00_0000), 0xFFFF_FFFF);
        assert_eq!(on_accent(0xFFFF_FFFF), 0xFF00_0000);
    }

    #[test]
    fn a_process_that_went_away_is_crashed_or_killed() {
        assert_eq!(gone(ProcessReason::Crashed), GoneReason::Crashed);
        assert_eq!(gone(ProcessReason::HungKilled), GoneReason::Killed);
        assert_eq!(gone(ProcessReason::Other(99)), GoneReason::Crashed);
    }

    #[test]
    fn a_pointer_arrives_with_every_field() {
        let out = pointer(&PointerEvent {
            kind: PointerKind::Down,
            x: 12.5,
            y: -3.0,
            button: 2,
            buttons: 3,
            modifiers: Mods(Mods::SHIFT | Mods::CTRL),
            click_count: 2,
        });
        assert_eq!(
            (out.kind, out.x, out.y),
            (vi::PointerKind::Down, 12.5, -3.0)
        );
        assert_eq!(out.button, Button::Right);
        assert_eq!(out.buttons, Buttons::LEFT | Buttons::RIGHT);
        assert_eq!(out.modifiers, Modifiers::SHIFT | Modifiers::CTRL);
        assert_eq!(out.click_count, 2);
    }

    #[test]
    fn a_wheel_keeps_its_unit_lines_and_precision() {
        let lines = wheel(&WheelEvent {
            x: 1.0,
            y: 2.0,
            dx: 0.0,
            dy: 1.0,
            unit: WheelUnit::Line,
            precise: false,
            modifiers: Mods(Mods::CTRL),
            phase: WheelPhase::None,
        });
        assert_eq!(lines.mode, WheelMode::Line);
        assert_eq!(
            (lines.dx, lines.dy),
            (0.0, 3.0),
            "a notch is three lines, as in Firefox"
        );
        assert_eq!(lines.ticks, Some((0.0, 1.0)), "and still one notch");
        assert_eq!(lines.lines, Some((0, 3)));
        assert_eq!(lines.modifiers, Modifiers::CTRL);
        let pixels = wheel(&WheelEvent {
            x: 0.0,
            y: 0.0,
            dx: -2.0,
            dy: -11.5,
            unit: WheelUnit::Pixel,
            precise: true,
            modifiers: Mods::default(),
            phase: WheelPhase::Began,
        });
        assert_eq!(pixels.phase, Phase::Began);
        assert_eq!(lines.phase, Phase::None, "a notch is not a gesture");
        assert_eq!(
            (pixels.mode, pixels.dx, pixels.dy),
            (WheelMode::Pixel, -2.0, -11.5)
        );
        assert!(pixels.flags.contains(WheelFlags::PRECISE));
    }

    #[test]
    fn a_key_arrives_with_its_names_location_repeat_and_text() {
        let out = key(&KeyEvent {
            down: true,
            key: "a".to_string(),
            code: "KeyA".to_string(),
            modifiers: Mods(Mods::ALT),
            repeat: true,
            location: 2,
            text: "a".to_string(),
        });
        assert_eq!(
            (out.key.as_str(), out.code.as_str(), out.text.as_str()),
            ("a", "KeyA", "a")
        );
        assert_eq!(out.action, KeyAction::Down);
        assert!(out.repeat);
        assert_eq!(out.location, KeyLocation::Right);
        assert_eq!(out.modifiers, Modifiers::ALT);
    }

    #[test]
    fn focus_is_the_windows_keyboard_or_a_move_inside_the_page() {
        assert!(matches!(
            focus(FocusAction::Activate),
            Focus::Window(WindowState::Active)
        ));
        assert!(matches!(
            focus(FocusAction::Deactivate),
            Focus::Window(WindowState::Visible)
        ));
        assert!(matches!(
            focus(FocusAction::Previous),
            Focus::Move(FocusMove::Previous)
        ));
        assert_eq!(
            window_state(false, true),
            WindowState::Hidden,
            "a hidden tab never has the keyboard"
        );
        assert_eq!(window_state(true, false), WindowState::Visible);
        assert_eq!(window_state(true, true), WindowState::Active);
    }

    #[test]
    fn the_menu_key_asks_for_the_menu_at_the_focus() {
        let at_focus = context_menu(None, Mods::default());
        assert_eq!(at_focus.origin, ContextMenuOrigin::Keyboard);
        let at_point = context_menu(Some((4.0, 5.0)), Mods(Mods::SHIFT));
        assert_eq!(
            (at_point.origin, at_point.x, at_point.y),
            (ContextMenuOrigin::Pointer, 4.0, 5.0)
        );
        assert_eq!(at_point.modifiers, Modifiers::SHIFT);
    }

    #[test]
    fn security_flags_and_states_map() {
        let secure = security(
            EngineSecurity::Secure,
            SecurityFlags::EV | SecurityFlags::MIXED_DISPLAY,
        );
        assert_eq!(secure.state, SecurityState::Secure);
        assert!(secure.extended_validation && secure.mixed_passive && !secure.mixed_active);
        assert_eq!(
            security(EngineSecurity::Local, SecurityFlags::empty()).state,
            SecurityState::Local
        );
        assert_eq!(
            security(EngineSecurity::Insecure, SecurityFlags::empty()).state,
            SecurityState::Insecure
        );
    }

    #[test]
    fn every_cursor_the_page_can_ask_for_is_a_cursor_the_view_draws() {
        assert_eq!(cursor(CursorKind::Text), ViewCursor::Text);
        assert_eq!(cursor(CursorKind::ResizeNwse), ViewCursor::NwseResize);
        assert_eq!(cursor(CursorKind::None), ViewCursor::Hidden);
        assert_eq!(cursor(CursorKind::Custom), ViewCursor::Default);
        assert_eq!(cursor(CursorKind::Other(99)), ViewCursor::Default);
    }
}
