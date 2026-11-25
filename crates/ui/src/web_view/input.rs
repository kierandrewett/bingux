//! What the host sends the page. These are the engine's own words (DOM names,
//! DOM button bits, the engine's modifier bits), so converting a bingux-ui
//! input loses nothing and the engine side is a field-for-field copy.

use crate::frame_view::{
    KeyInput, PointerAction, PointerInput, TextInput, TouchInput, WheelDelta, WheelInput, WheelPhase,
};
use freya::prelude::{MouseButton, Modifiers, TouchPhase};

/// The modifier keys and locks held, as the engine's `VV_MOD_*` bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods(pub u32);

impl Mods {
    pub const SHIFT: u32 = 1;
    pub const CTRL: u32 = 2;
    pub const ALT: u32 = 4;
    pub const META: u32 = 8;
    pub const ALTGR: u32 = 16;
    pub const CAPS_LOCK: u32 = 32;
    pub const NUM_LOCK: u32 = 64;

    pub fn from_freya(modifiers: Modifiers) -> Self {
        let mut bits = 0;
        for (modifier, bit) in [
            (Modifiers::SHIFT, Self::SHIFT),
            (Modifiers::CONTROL, Self::CTRL),
            (Modifiers::ALT, Self::ALT),
            (Modifiers::META, Self::META),
            (Modifiers::ALT_GRAPH, Self::ALTGR),
            (Modifiers::CAPS_LOCK, Self::CAPS_LOCK),
            (Modifiers::NUM_LOCK, Self::NUM_LOCK),
        ] {
            if modifiers.contains(modifier) {
                bits |= bit;
            }
        }
        Self(bits)
    }

    pub const fn has(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// What a pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Move,
    Down,
    Up,
    Enter,
    Leave,
}

/// A mouse button, as the DOM's `MouseEvent.button`.
pub fn dom_button(button: MouseButton) -> Option<u32> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        MouseButton::Back => Some(3),
        MouseButton::Forward => Some(4),
        MouseButton::Other(_) => None,
    }
}

/// A mouse event. `x` and `y` are logical pixels from the view's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub x: f64,
    pub y: f64,
    /// `MouseEvent.button` of a `Down` or `Up`.
    pub button: u32,
    /// `MouseEvent.buttons` held after the event.
    pub buttons: u32,
    pub modifiers: Mods,
    pub click_count: u32,
}

/// The unit of a wheel's deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelUnit {
    Pixel,
    Line,
}

/// A wheel or touchpad scroll. Positive `dy` scrolls the page down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub unit: WheelUnit,
    /// A high resolution device (a touchpad): the deltas are exact.
    pub precise: bool,
    pub modifiers: Mods,
    /// Where a touchpad gesture is: the engine scrolls with its own momentum
    /// and overscroll from the start and the lift of the fingers.
    pub phase: WheelPhase,
}

/// A key going down or up, in the page's own terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub down: bool,
    /// `KeyboardEvent.key`.
    pub key: String,
    /// `KeyboardEvent.code`.
    pub code: String,
    pub modifiers: Mods,
    pub repeat: bool,
    /// `KeyboardEvent.location`: 0 standard, 1 left, 2 right, 3 numpad.
    pub location: u32,
    /// What the key types with the layout, or empty.
    pub text: String,
}

/// Text from an input method.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEvent {
    /// Finished text.
    Commit(String),
    /// Text being composed. `caret` is the byte range of the input method's
    /// cursor inside `text`. Empty `text` ends the composition.
    Preedit {
        text: String,
        caret: Option<(usize, usize)>,
    },
}

/// What a finger did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchKind {
    Down,
    Move,
    Up,
    Cancel,
}

/// One contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchEvent {
    pub kind: TouchKind,
    pub id: u64,
    pub x: f64,
    pub y: f64,
}

/// An editing command for the focused editor or the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Cut,
    Copy,
    Paste,
    PastePlain,
    SelectAll,
    Undo,
    Redo,
    Delete,
}

/// Where the keyboard focus goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusAction {
    /// The window is the active one: the page may show a caret.
    Activate,
    Deactivate,
    /// Tab: into the page, to the next focusable element.
    Next,
    /// Shift+Tab.
    Previous,
    First,
    Last,
}

/// Everything the host can send a page.
#[derive(Clone, Debug, PartialEq)]
pub enum PageInput {
    Pointer(PointerEvent),
    Wheel(WheelEvent),
    Key(KeyEvent),
    Text(TextEvent),
    Touch(TouchEvent),
    Edit(EditCommand),
    Focus(FocusAction),
    /// A request for the page's context menu (`contextmenu`): at a point, or
    /// at the focused element when `at` is `None` (the menu key).
    ContextMenu {
        at: Option<(f64, f64)>,
        modifiers: Mods,
    },
}

/// `KeyboardEvent.location` for a physical key name.
pub fn key_location(code: &str) -> u32 {
    if code.starts_with("Numpad") {
        3
    } else if code.ends_with("Left") && is_modifier_code(code) {
        1
    } else if code.ends_with("Right") && is_modifier_code(code) {
        2
    } else {
        0
    }
}

fn is_modifier_code(code: &str) -> bool {
    ["Shift", "Control", "Alt", "Meta"]
        .iter()
        .any(|name| code.starts_with(name))
}

impl PageInput {
    /// A pointer event of the frame view. A button that has no DOM number
    /// (an extra button) is not sent: the page has no name for it.
    pub fn from_pointer(input: &PointerInput) -> Option<Self> {
        let kind = match input.action {
            PointerAction::Move => PointerKind::Move,
            PointerAction::Down => PointerKind::Down,
            PointerAction::Up => PointerKind::Up,
            PointerAction::Enter => PointerKind::Enter,
            PointerAction::Leave => PointerKind::Leave,
        };
        let button = match input.button {
            Some(button) => dom_button(button)?,
            None => 0,
        };
        Some(Self::Pointer(PointerEvent {
            kind,
            x: f64::from(input.position.0),
            y: f64::from(input.position.1),
            button,
            buttons: u32::from(input.buttons.0),
            modifiers: Mods::from_freya(input.modifiers),
            click_count: input.click_count,
        }))
    }

    pub fn from_wheel(input: &WheelInput) -> Self {
        let (dx, dy, unit, precise) = match input.delta {
            WheelDelta::Pixels { x, y } => (x, y, WheelUnit::Pixel, true),
            WheelDelta::Lines { x, y } => (x, y, WheelUnit::Line, false),
        };
        Self::Wheel(WheelEvent {
            x: f64::from(input.position.0),
            y: f64::from(input.position.1),
            dx: f64::from(dx),
            dy: f64::from(dy),
            unit,
            precise,
            modifiers: Mods::from_freya(input.modifiers),
            phase: input.phase,
        })
    }

    pub fn from_key(input: &KeyInput) -> Self {
        let code = input.code_string();
        Self::Key(KeyEvent {
            down: input.pressed,
            key: input.key_string(),
            location: key_location(&code),
            code,
            modifiers: Mods::from_freya(input.modifiers),
            repeat: input.repeat,
            text: input.text.clone().unwrap_or_default(),
        })
    }

    pub fn from_text(input: &TextInput) -> Self {
        Self::Text(match input {
            TextInput::Commit(text) => TextEvent::Commit(text.clone()),
            TextInput::Preedit { text, cursor } => TextEvent::Preedit {
                text: text.clone(),
                caret: *cursor,
            },
        })
    }

    /// A touch of the frame view.
    pub fn from_touch(input: &TouchInput) -> Self {
        let kind = match input.phase {
            TouchPhase::Started => TouchKind::Down,
            TouchPhase::Moved => TouchKind::Move,
            TouchPhase::Ended => TouchKind::Up,
            TouchPhase::Cancelled => TouchKind::Cancel,
        };
        Self::Touch(TouchEvent {
            kind,
            id: input.id,
            x: f64::from(input.position.0),
            y: f64::from(input.position.1),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::frame_view::PointerButtons;
    use freya::prelude::{Code, Key, NamedKey};

    use super::*;

    #[test]
    fn a_pointer_keeps_its_position_buttons_modifiers_and_click_count() {
        let input = PointerInput {
            action: PointerAction::Down,
            position: (12.5, -3.0),
            button: Some(MouseButton::Right),
            buttons: PointerButtons(PointerButtons::RIGHT | PointerButtons::LEFT),
            modifiers: Modifiers::SHIFT | Modifiers::CONTROL,
            click_count: 2,
        };
        let Some(PageInput::Pointer(event)) = PageInput::from_pointer(&input) else {
            panic!("a pointer event");
        };
        assert_eq!(event.kind, PointerKind::Down);
        assert_eq!((event.x, event.y), (12.5, -3.0), "outside the view stays negative");
        assert_eq!(event.button, 2, "the DOM number of the right button");
        assert_eq!(event.buttons, 3);
        assert_eq!(event.modifiers, Mods(Mods::SHIFT | Mods::CTRL));
        assert_eq!(event.click_count, 2);
    }

    #[test]
    fn an_extra_button_is_not_sent_as_another() {
        let input = PointerInput {
            action: PointerAction::Down,
            position: (1.0, 1.0),
            button: Some(MouseButton::Other(9)),
            buttons: PointerButtons(0),
            modifiers: Modifiers::default(),
            click_count: 1,
        };
        assert_eq!(PageInput::from_pointer(&input), None);
    }

    #[test]
    fn the_wheel_keeps_its_unit_and_sign() {
        let lines = WheelInput {
            delta: WheelDelta::Lines { x: 0.0, y: 3.0 },
            position: (4.0, 5.0),
            modifiers: Modifiers::CONTROL,
            phase: WheelPhase::None,
        };
        let PageInput::Wheel(event) = PageInput::from_wheel(&lines) else {
            panic!("a wheel event");
        };
        assert_eq!((event.unit, event.dy, event.precise), (WheelUnit::Line, 3.0, false));
        assert!(event.modifiers.has(Mods::CTRL));
        let pixels = WheelInput {
            delta: WheelDelta::Pixels { x: -2.0, y: -11.5 },
            position: (0.0, 0.0),
            modifiers: Modifiers::default(),
            phase: WheelPhase::Changed,
        };
        let PageInput::Wheel(event) = PageInput::from_wheel(&pixels) else {
            panic!("a wheel event");
        };
        assert_eq!((event.unit, event.dx, event.dy, event.precise), (WheelUnit::Pixel, -2.0, -11.5, true));
        assert_eq!(event.phase, WheelPhase::Changed, "the gesture phase goes through");
    }

    #[test]
    fn a_key_keeps_its_dom_names_location_repeat_and_text() {
        let input = KeyInput {
            pressed: true,
            key: Key::Character("a".to_string()),
            code: Code::KeyA,
            modifiers: Modifiers::default(),
            repeat: true,
            text: Some("a".to_string()),
        };
        let PageInput::Key(event) = PageInput::from_key(&input) else {
            panic!("a key event");
        };
        assert_eq!((event.key.as_str(), event.code.as_str()), ("a", "KeyA"));
        assert!(event.down && event.repeat);
        assert_eq!(event.text, "a");
        assert_eq!(event.location, 0);
        let shift = KeyInput {
            pressed: false,
            key: Key::Named(NamedKey::Shift),
            code: Code::ShiftRight,
            modifiers: Modifiers::SHIFT,
            repeat: false,
            text: None,
        };
        let PageInput::Key(event) = PageInput::from_key(&shift) else {
            panic!("a key event");
        };
        assert_eq!(event.location, 2, "the right shift key");
        assert_eq!(event.text, "");
    }

    #[test]
    fn locations_follow_the_physical_key() {
        assert_eq!(key_location("Numpad7"), 3);
        assert_eq!(key_location("ControlLeft"), 1);
        assert_eq!(key_location("AltRight"), 2);
        assert_eq!(key_location("ArrowLeft"), 0, "an arrow is not a left key");
        assert_eq!(key_location("Enter"), 0);
    }

    #[test]
    fn text_from_an_input_method_keeps_the_preedit_cursor() {
        let input = TextInput::Preedit {
            text: "ni".to_string(),
            cursor: Some((1, 2)),
        };
        assert_eq!(
            PageInput::from_text(&input),
            PageInput::Text(TextEvent::Preedit {
                text: "ni".to_string(),
                caret: Some((1, 2)),
            })
        );
        assert_eq!(
            PageInput::from_text(&TextInput::Commit("\u{4f60}".to_string())),
            PageInput::Text(TextEvent::Commit("\u{4f60}".to_string()))
        );
    }
}
