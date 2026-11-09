//! How a preview is framed.

/// The frames one preview content can sit in. The content is the same
/// component in all of them; see [`PreviewHost`](super::PreviewHost).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A modal card over a scrim: Quick Look, Sushi.
    Popup,
    /// A docked column at the right edge of the view that opened it; the view
    /// stays usable and the pane follows its selection.
    Pane,
    /// The whole window, with the chrome out of the way until the pointer
    /// moves.
    Fullscreen,
    /// The content of an application's own window (the Preview app): the
    /// owner draws the title line, Space and Escape do not close it, and Enter
    /// and the Properties key leave it open.
    Window,
}

/// How much a viewer does for the person, which the frame decides.
///
/// A frame that is a quick look (a popup, a pane, a full-screen sheet) shows
/// the content and little else: the file is a click or a keystroke from
/// being gone, and a pane follows a selection. The Preview application is a
/// dedicated window for one file at a time, so every viewer adds its own tools
/// there: a toolbar, a second kind of reading, an action on the file.
///
/// A viewer asks [`Stage::depth`](super::viewers::Stage::depth) and never
/// compares a [`Mode`] itself, so what a frame means is decided here once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// The content, and the keys that read it.
    Glance,
    /// The content and the viewer's own tools.
    Dedicated,
}

impl Mode {
    /// How much a viewer does in this frame.
    pub fn depth(self) -> Depth {
        match self {
            Mode::Window => Depth::Dedicated,
            Mode::Popup | Mode::Pane | Mode::Fullscreen => Depth::Glance,
        }
    }

    /// The modes a window can offer. A compact window (a phone) has no room
    /// for a pane beside a list, so it only opens a full-window sheet, which
    /// is the popup at the window's size.
    pub fn available(self, compact: bool) -> Mode {
        match (self, compact) {
            (Mode::Pane, true) => Mode::Popup,
            (mode, _) => mode,
        }
    }

    /// What the header's switch button does from here: a popup docks, a pane
    /// undocks into a popup.
    pub fn switched(self) -> Mode {
        match self {
            Mode::Popup | Mode::Fullscreen | Mode::Window => Mode::Pane,
            Mode::Pane => Mode::Popup,
        }
    }

    /// The name of the mode in the remembered state.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Popup => "popup",
            Mode::Pane => "pane",
            Mode::Fullscreen => "fullscreen",
            Mode::Window => "window",
        }
    }

    pub fn from_name(name: &str) -> Option<Mode> {
        match name {
            "popup" => Some(Mode::Popup),
            "pane" => Some(Mode::Pane),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_never_gets_a_pane() {
        assert_eq!(Mode::Pane.available(true), Mode::Popup);
        assert_eq!(Mode::Pane.available(false), Mode::Pane);
        assert_eq!(Mode::Fullscreen.available(true), Mode::Fullscreen);
    }

    #[test]
    fn only_an_applications_own_window_is_dedicated() {
        assert_eq!(Mode::Window.depth(), Depth::Dedicated);
        for mode in [Mode::Popup, Mode::Pane, Mode::Fullscreen] {
            assert_eq!(mode.depth(), Depth::Glance, "{mode:?}");
        }
    }

    #[test]
    fn the_switch_docks_and_undocks() {
        assert_eq!(Mode::Popup.switched(), Mode::Pane);
        assert_eq!(Mode::Pane.switched(), Mode::Popup);
        assert_eq!(Mode::Fullscreen.switched(), Mode::Pane);
    }

    #[test]
    fn only_the_two_chosen_modes_are_remembered() {
        assert_eq!(Mode::from_name(Mode::Popup.name()), Some(Mode::Popup));
        assert_eq!(Mode::from_name(Mode::Pane.name()), Some(Mode::Pane));
        assert_eq!(Mode::from_name("fullscreen"), None);
    }
}
