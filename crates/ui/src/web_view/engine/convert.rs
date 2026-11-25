//! Field for field: the host's words for input and the engine's.
//!
//! [`PageInput`] already uses the engine's own names and bit values, so each
//! conversion copies fields and loses none. The tests are the proof that every
//! field arrives.

use crate::frame_view::{CursorKind as ViewCursor, WheelPhase};
use vivarium as vv;

use crate::web_view::input::{
    EditCommand, FocusAction, KeyEvent, PageInput, PointerEvent, PointerKind, TextEvent, TouchEvent, TouchKind,
    WheelEvent, WheelUnit,
};
use crate::web_view::page::{GoneReason, Security, SecurityState};

fn modifiers(bits: u32) -> vv::Modifiers {
    vv::Modifiers::from_bits_truncate(bits)
}

/// The engine's clock, with which input is stamped: what the page sees as the
/// event's `timeStamp` is when the person acted, not when the engine took it
/// from the queue.
fn now() -> u64 {
    #[cfg(target_os = "linux")]
    {
        vv::now_ns()
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

pub(super) fn pointer(event: &PointerEvent) -> vv::PointerEvent {
    let kind = match event.kind {
        PointerKind::Move => vv::PointerKind::Move,
        PointerKind::Down => vv::PointerKind::Down,
        PointerKind::Up => vv::PointerKind::Up,
        PointerKind::Enter => vv::PointerKind::Enter,
        PointerKind::Leave => vv::PointerKind::Leave,
    };
    let mut out = vv::PointerEvent::mouse(kind, event.x, event.y)
        .with_button(vv::Button::from_raw(event.button))
        .with_buttons(vv::Buttons::from_bits_truncate(event.buttons))
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
        if value <= 0.039_28 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    // Contrast with white is 1.05 / (luminance + 0.05): at least 3 below 0.30.
    if luminance <= 0.30 { 0xFFFF_FFFF } else { 0xFF00_0000 }
}

/// The engine-wide environment for the host's appearance. A field goes in
/// only when `announced` says the engine implements it (`env.<field>`): the
/// library refuses a whole call that names one it does not, and the answer
/// would be `Unsupported` for the fields that do work too.
pub(super) fn environment(environment: &crate::web_view::environment::PageEnvironment, announced: &dyn Fn(&str) -> bool) -> vv::Environment {
    let mut out = vv::Environment::default();
    if announced("color_scheme") {
        out.color_scheme = Some(if environment.dark() { vv::ColorScheme::Dark } else { vv::ColorScheme::Light });
    }
    if announced("reduced_motion") {
        out.reduced_motion = Some(environment.reduced_motion);
    }
    if announced("contrast") {
        out.contrast = Some(if environment.more_contrast { vv::Contrast::More } else { vv::Contrast::NoPreference });
    }
    if announced("text_scale") {
        out.text_scale = Some(environment.text_scale);
    }
    if announced("accent") {
        out.accent = Some((environment.accent, on_accent(environment.accent)));
    }
    if announced("locales") && !environment.locales.is_empty() {
        out.locales = Some(environment.locales.clone());
    }
    out
}

/// What a page sees for one notch of a wheel: three lines, as Firefox on GTK
/// reports (`deltaY` 3 in `DOM_DELTA_LINE`, which scrolls about 57 px at the
/// default font). The host counts notches (a hi-res wheel's fraction stays);
/// the engine is told lines, so a notch scrolls what it scrolls in Firefox.
const LINES_PER_NOTCH: f64 = 3.0;

pub(super) fn wheel(event: &WheelEvent) -> vv::WheelEvent {
    let mut out = vv::WheelEvent::pixels(event.x, event.y, event.dx, event.dy).with_modifiers(modifiers(event.modifiers.0));
    match event.unit {
        WheelUnit::Pixel => {}
        WheelUnit::Line => {
            out.mode = vv::WheelMode::Line;
            out.dx = event.dx * LINES_PER_NOTCH;
            out.dy = event.dy * LINES_PER_NOTCH;
            out.ticks = Some((event.dx, event.dy));
            out.lines = Some(((event.dx * LINES_PER_NOTCH).round() as i32, (event.dy * LINES_PER_NOTCH).round() as i32));
        }
    }
    out.phase = match event.phase {
        WheelPhase::None => vv::Phase::None,
        WheelPhase::Began => vv::Phase::Began,
        WheelPhase::Changed => vv::Phase::Changed,
        WheelPhase::Ended => vv::Phase::Ended,
        WheelPhase::Cancelled => vv::Phase::Cancelled,
    };
    if event.precise {
        out.flags |= vv::WheelFlags::PRECISE;
    }
    out.time_ns = now();
    out
}

pub(super) fn key(event: &KeyEvent) -> vv::KeyEvent {
    let action = if event.down { vv::KeyAction::Down } else { vv::KeyAction::Up };
    let mut out = vv::KeyEvent::new(action, event.key.clone(), event.code.clone())
        .with_modifiers(modifiers(event.modifiers.0))
        .with_text(event.text.clone());
    out.repeat = event.repeat;
    out.location = vv::KeyLocation::from_raw(event.location);
    out.time_ns = now();
    out
}

pub(super) fn touch(event: &TouchEvent) -> vv::TouchEvent {
    let kind = match event.kind {
        TouchKind::Down => vv::TouchKind::Down,
        TouchKind::Move => vv::TouchKind::Move,
        TouchKind::Up => vv::TouchKind::Up,
        TouchKind::Cancel => vv::TouchKind::Cancel,
    };
    let mut out = vv::TouchEvent::finger(kind, event.id as u32, event.x, event.y);
    out.time_ns = now();
    out
}

pub(super) fn edit(command: EditCommand) -> vv::EditCommand {
    match command {
        EditCommand::Cut => vv::EditCommand::Cut,
        EditCommand::Copy => vv::EditCommand::Copy,
        EditCommand::Paste => vv::EditCommand::Paste,
        EditCommand::PastePlain => vv::EditCommand::PastePlain,
        EditCommand::SelectAll => vv::EditCommand::SelectAll,
        EditCommand::Undo => vv::EditCommand::Undo,
        EditCommand::Redo => vv::EditCommand::Redo,
        EditCommand::Delete => vv::EditCommand::Delete,
    }
}

pub(super) fn focus(action: FocusAction) -> vv::FocusAction {
    match action {
        FocusAction::Activate => vv::FocusAction::Activate,
        FocusAction::Deactivate => vv::FocusAction::Deactivate,
        FocusAction::Next => vv::FocusAction::Next,
        FocusAction::Previous => vv::FocusAction::Previous,
        FocusAction::First => vv::FocusAction::First,
        FocusAction::Last => vv::FocusAction::Last,
    }
}

pub(super) fn context_menu(at: Option<(f64, f64)>, held: crate::web_view::input::Mods) -> vv::ContextMenuEvent {
    match at {
        Some((x, y)) => {
            let mut event = vv::ContextMenuEvent::at(x, y);
            event.modifiers = modifiers(held.0);
            event
        }
        None => vv::ContextMenuEvent {
            origin: vv::ContextMenuOrigin::Keyboard,
            modifiers: modifiers(held.0),
            x: 0.0,
            y: 0.0,
        },
    }
}

/// What a [`PageInput`] is as a composition: a preedit that is empty ends it.
pub(super) fn composition(text: &str, caret: Option<(usize, usize)>) -> vv::CompositionEvent {
    if text.is_empty() {
        return vv::CompositionEvent::new(vv::CompositionKind::Cancel, "");
    }
    let mut event = vv::CompositionEvent::new(vv::CompositionKind::Update, text);
    event.caret = caret.map(|(start, end)| vv::Caret {
        offset: start as u32,
        length: end.saturating_sub(start) as u32,
    });
    event
}

/// Send `input` to `session`, the one call it is.
pub(super) fn send(session: &vv::Session, input: &PageInput) -> vv::Result<()> {
    match input {
        PageInput::Pointer(event) => session.pointer(&pointer(event)),
        PageInput::Wheel(event) => session.wheel(&wheel(event)),
        PageInput::Key(event) => session.key(&key(event)),
        PageInput::Text(TextEvent::Commit(text)) => session.text(text),
        PageInput::Text(TextEvent::Preedit { text, caret }) => session.composition(&composition(text, *caret)),
        PageInput::Touch(event) => session.touch(&touch(event)),
        PageInput::Edit(command) => session.edit(edit(*command)),
        PageInput::Focus(action) => session.focus(focus(*action)),
        PageInput::ContextMenu { at, modifiers } => session.context_menu(&context_menu(*at, *modifiers)),
    }
}

pub(super) fn security(state: vv::SecurityState, flags: vv::SecurityFlags) -> Security {
    Security {
        state: match state {
            vv::SecurityState::Secure => SecurityState::Secure,
            vv::SecurityState::Broken => SecurityState::Broken,
            vv::SecurityState::Local => SecurityState::Local,
            _ => SecurityState::Insecure,
        },
        mixed_active: flags.contains(vv::SecurityFlags::MIXED_ACTIVE),
        mixed_passive: flags.contains(vv::SecurityFlags::MIXED_PASSIVE),
        extended_validation: flags.contains(vv::SecurityFlags::EV),
    }
}

pub(super) fn gone(reason: vv::ProcessReason) -> GoneReason {
    match reason {
        vv::ProcessReason::Killed | vv::ProcessReason::HungKilled => GoneReason::Killed,
        vv::ProcessReason::CleanExit => GoneReason::CleanExit,
        // A process that ran out of memory crashed, as far as the tab is concerned.
        _ => GoneReason::Crashed,
    }
}

/// The pointer shape of the engine as the view draws it. A cursor that is an
/// image has no name to draw it by, so it is the arrow.
pub(super) fn cursor(kind: vv::CursorKind) -> ViewCursor {
    match kind {
        vv::CursorKind::Pointer => ViewCursor::Pointer,
        vv::CursorKind::Text => ViewCursor::Text,
        vv::CursorKind::Wait => ViewCursor::Wait,
        vv::CursorKind::Progress => ViewCursor::Progress,
        vv::CursorKind::Help => ViewCursor::Help,
        vv::CursorKind::Crosshair => ViewCursor::Crosshair,
        vv::CursorKind::Move => ViewCursor::Move,
        vv::CursorKind::Grab => ViewCursor::Grab,
        vv::CursorKind::Grabbing => ViewCursor::Grabbing,
        vv::CursorKind::NotAllowed => ViewCursor::NotAllowed,
        vv::CursorKind::ResizeNs => ViewCursor::NsResize,
        vv::CursorKind::ResizeEw => ViewCursor::EwResize,
        vv::CursorKind::ResizeNesw => ViewCursor::NeswResize,
        vv::CursorKind::ResizeNwse => ViewCursor::NwseResize,
        vv::CursorKind::ZoomIn => ViewCursor::ZoomIn,
        vv::CursorKind::ZoomOut => ViewCursor::ZoomOut,
        vv::CursorKind::None => ViewCursor::Hidden,
        _ => ViewCursor::Default,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_hosts_appearance_becomes_the_engines_environment() {
        use super::*;
        use crate::web_view::environment::{PageEnvironment, PageScheme};
        let host = PageEnvironment {
            scheme: PageScheme::Dark,
            accent: 0xFF11_2233,
            reduced_motion: true,
            more_contrast: false,
            text_scale: 1.5,
            locales: vec!["de-DE".to_string()],
        };
        let engine = environment(&host, &|_| true);
        assert_eq!(engine.color_scheme, Some(vv::ColorScheme::Dark));
        assert_eq!(engine.reduced_motion, Some(true));
        assert_eq!(engine.contrast, Some(vv::Contrast::NoPreference));
        assert_eq!(engine.text_scale, Some(1.5));
        assert_eq!(engine.accent, Some((0xFF11_2233, 0xFFFF_FFFF)), "white on a dark accent");
        assert_eq!(engine.locales.as_deref(), Some(&["de-DE".to_string()][..]));
        assert!(environment(&PageEnvironment::default(), &|_| true).locales.is_none(), "no locales is no change");
    }

    #[test]
    fn a_field_the_engine_does_not_announce_is_not_sent() {
        use super::*;
        use crate::web_view::environment::PageEnvironment;
        // The engine of today: colour scheme, accent and palette only.
        let today = |field: &str| matches!(field, "color_scheme" | "accent" | "palette");
        let engine = environment(&PageEnvironment { reduced_motion: true, text_scale: 2.0, locales: vec!["fr".into()], ..PageEnvironment::default() }, &today);
        assert!(engine.color_scheme.is_some() && engine.accent.is_some());
        assert_eq!((engine.reduced_motion, engine.contrast, engine.text_scale, engine.locales), (None, None, None, None));
        assert!(environment(&PageEnvironment::default(), &|_| false).is_empty(), "nothing announced, nothing to send");
    }

    #[test]
    fn the_text_on_the_accent_is_the_one_that_can_be_read() {
        assert_eq!(on_accent(0xFF35_84E4), 0xFFFF_FFFF, "the shell's blue takes white");
        assert_eq!(on_accent(0xFF26_A269), 0xFFFF_FFFF, "a green takes white");
        assert_eq!(on_accent(0xFFF6_D32D), 0xFF00_0000, "a yellow takes black");
        assert_eq!(on_accent(0xFF00_0000), 0xFFFF_FFFF);
        assert_eq!(on_accent(0xFFFF_FFFF), 0xFF00_0000);
    }

    #[test]
    fn a_process_that_went_away_is_crashed_killed_or_gone_clean() {
        assert_eq!(gone(vv::ProcessReason::Crashed), GoneReason::Crashed);
        assert_eq!(gone(vv::ProcessReason::Oom), GoneReason::Crashed);
        assert_eq!(gone(vv::ProcessReason::HungKilled), GoneReason::Killed);
        assert_eq!(gone(vv::ProcessReason::Killed), GoneReason::Killed);
        assert_eq!(gone(vv::ProcessReason::CleanExit), GoneReason::CleanExit);
    }

    use super::*;
    use crate::web_view::input::Mods;

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
        assert_eq!((out.kind, out.x, out.y), (vv::PointerKind::Down, 12.5, -3.0));
        assert_eq!(out.button, vv::Button::Right);
        assert_eq!(out.buttons, vv::Buttons::LEFT | vv::Buttons::RIGHT);
        assert_eq!(out.modifiers, vv::Modifiers::SHIFT | vv::Modifiers::CTRL);
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
        assert_eq!(lines.mode, vv::WheelMode::Line);
        assert_eq!((lines.dx, lines.dy), (0.0, 3.0), "a notch is three lines, as in Firefox");
        assert_eq!(lines.ticks, Some((0.0, 1.0)), "and still one notch");
        assert_eq!(lines.lines, Some((0, 3)));
        assert_eq!(lines.modifiers, vv::Modifiers::CTRL);
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
        assert_eq!(pixels.phase, vv::Phase::Began);
        assert_eq!(lines.phase, vv::Phase::None, "a notch is not a gesture");
        assert_eq!((pixels.mode, pixels.dx, pixels.dy), (vv::WheelMode::Pixel, -2.0, -11.5));
        assert!(pixels.flags.contains(vv::WheelFlags::PRECISE));
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
        assert_eq!((out.key.as_str(), out.code.as_str(), out.text.as_str()), ("a", "KeyA", "a"));
        assert_eq!(out.action, vv::KeyAction::Down);
        assert!(out.repeat);
        assert_eq!(out.location, vv::KeyLocation::Right);
        assert_eq!(out.modifiers, vv::Modifiers::ALT);
    }

    #[test]
    fn a_preedit_is_an_update_with_its_cursor_and_an_empty_one_ends_it() {
        let update = composition("ni", Some((1, 2)));
        assert_eq!(update.kind, vv::CompositionKind::Update);
        assert_eq!(update.caret, Some(vv::Caret { offset: 1, length: 1 }));
        assert_eq!(composition("", None).kind, vv::CompositionKind::Cancel);
    }

    #[test]
    fn the_menu_key_asks_for_the_menu_at_the_focus() {
        let at_focus = context_menu(None, Mods::default());
        assert_eq!(at_focus.origin, vv::ContextMenuOrigin::Keyboard);
        let at_point = context_menu(Some((4.0, 5.0)), Mods(Mods::SHIFT));
        assert_eq!((at_point.origin, at_point.x, at_point.y), (vv::ContextMenuOrigin::Pointer, 4.0, 5.0));
        assert_eq!(at_point.modifiers, vv::Modifiers::SHIFT);
    }

    #[test]
    fn security_flags_and_states_map() {
        let secure = security(vv::SecurityState::Secure, vv::SecurityFlags::EV | vv::SecurityFlags::MIXED_PASSIVE);
        assert_eq!(secure.state, SecurityState::Secure);
        assert!(secure.extended_validation && secure.mixed_passive && !secure.mixed_active);
        assert_eq!(security(vv::SecurityState::Local, vv::SecurityFlags::empty()).state, SecurityState::Local);
        assert_eq!(security(vv::SecurityState::Insecure, vv::SecurityFlags::empty()).state, SecurityState::Insecure);
    }

    #[test]
    fn every_cursor_the_page_can_ask_for_is_a_cursor_the_view_draws() {
        assert_eq!(cursor(vv::CursorKind::Text), ViewCursor::Text);
        assert_eq!(cursor(vv::CursorKind::ResizeNwse), ViewCursor::NwseResize);
        assert_eq!(cursor(vv::CursorKind::None), ViewCursor::Hidden);
        assert_eq!(cursor(vv::CursorKind::Custom), ViewCursor::Default);
        assert_eq!(cursor(vv::CursorKind::Other(99)), ViewCursor::Default);
    }
}
