//! The Bingux icon set, embedded in the binary at compile time.
//!
//! The icons are the rounded Phosphor fork of the chat app (`@chat/icons`),
//! snapped to a 16 px pixel grid so they render as crisply as GNOME's symbolic
//! icons. The SVG files live in `icons/` and are generated from
//! `scripts/icons/src` by `make icons`; see `icons/COPYING` for the licence.
//! Each icon is one fill path, recoloured at draw time so it follows the
//! active theme. A level icon (Wi-Fi signal, battery) has a second path with
//! `fill-opacity` for the parts that are off; the tint keeps that opacity.

use freya::prelude::*;

/// Every bundled icon, named after what it draws (`scripts/icons/aliases.tsv`):
/// `Calendar`, `Shield`, `ChevronRight`. The QML shell asks for the same names
/// (`calendar-symbolic`); [`Icon::from_freedesktop`] finds an icon by its shape
/// name or by a freedesktop name it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    WindowSidebar,
    Laptop,
    Gear,
    Monitor,
    Cross,
    Minus,
    Square,
    SquaresOverlapLeft,
    Palette,
    MonitorDock,
    Magnifier,
    Funnel,
    Wifi,
    Bluetooth,
    SpeakerWave2,
    Battery100,
    Keyboard,
    Mouse,
    PersonCircle,
    Clock,
    People,
    CircleInfo,
    ArrowCw,
    Trash,
    Check,
    Plus,
    /// Two arrows, one each way: swap two values (the units of a converter).
    ArrowsLeftRight,
    ChevronDown,
    ChevronUp,
    CircleCross,
    TriangleExclamation,
    SquaresOverlap,
    Folder,
    FolderOpen,
    LinesThree,
    DotsThree,
    Cube,
    DotsNine,
    Headphones,
    SpeakerBox,
    Gamepad,
    Phone,
    Pause,
    CursorClick,
    Wifi0,
    Wifi1,
    Wifi2,
    /// The padlock: a secured network, and anything locked or not permitted
    /// (a folder the person has no permission to read). It is the set's only
    /// padlock.
    Padlock,
    Network,
    Shield,
    Airplane,
    Router,
    WifiSlash,
    WifiCross,
    NetworkCross,
    Microphone,
    SpeakerCross,
    Speaker,
    SpeakerWave1,
    Webcam,
    Sun,
    MoonStars,
    Moon,
    Leaf,
    Gauge,
    Bolt,
    Power,
    Battery0,
    Battery10,
    Battery20,
    Battery30,
    Battery40,
    Battery50,
    Battery60,
    Battery70,
    Battery80,
    Battery90,
    Eye,
    EyeSlash,
    SquarePencil,
    /// A pencil: edit something (a path, a connection, an access point). `SquarePencil` is only for editing a document.
    Pencil,
    IdCard,
    StarFilled,
    Star,
    /// Play.
    Play,
    /// Stop: the Files toolbar's Reload button while a folder loads.
    Stop,
    /// Full screen.
    CornersOut,
    /// Leave full screen.
    CornersIn,
    /// Zoom in.
    MagnifierPlus,
    /// Zoom out.
    MagnifierMinus,
    /// Fit to the window.
    FrameCorners,
    /// Rotate counter-clockwise.
    ArrowCcw,
    /// Crop a picture.
    Crop,
    /// Subtitles.
    Subtitles,
    Key,
    SquareArrowUp,
    FolderArrowDown,
    HandTap,
    Sliders,
    ListBullets,
    Usb,
    Printer,
    Tablet,
    Server,
    Person,
    /// An arrow pointing up and right: where the computer is (Weather's My Location).
    Arrowhead,
    Envelope,
    MusicNotes,
    Calculator,
    Globe,
    Calendar,
    CircleQuestion,
    SquaresFour,
    Window,
    BellSlash,
    /// A coffee cup: keep the computer awake. Not a padlock: something locked
    /// is [`Icon::Padlock`].
    CoffeeCup,
    Bookmark,
    PuzzlePiece,
    VideoCamera,
    Image,
    House,
    FolderFileText,
    FolderMusicNote,
    FolderImage,
    FolderPlay,
    HardDrive,
    FileText,
    FileAudio,
    Package,
    FolderPlus,
    /// A folder drawn in dashes: a folder that is not there (not found).
    FolderDashed,
    Eject,
    Disc,
    FolderFile,
    FolderPerson,
    FolderMonitor,
    SquareArrowUpRight,
    FileDoc,
    DotsSix,
    /// A PDF file.
    FilePdf,
    /// A spreadsheet.
    Table,
    /// A presentation.
    Easel,
    /// A font file.
    SquareLettersAa,
    /// Source code or a script.
    FileCode,
    /// A file of no known kind.
    File,
    /// The badge on the icon of a symbolic link.
    CircleArrowUpRight,
    /// Recording: the dot of a recording indicator.
    Record,
    /// Previous track.
    SkipBack,
    /// Next track.
    SkipForward,
    /// Back 10 seconds.
    Rewind,
    /// Forward 10 seconds.
    FastForward,
    /// Microphone muted.
    MicrophoneSlash,
    /// Connected, kind of network unknown.
    ArrowsUpDown,
    /// Devices on one local network.
    Devices,
    /// Screen sharing in progress.
    MonitorArrowUp,
    /// Log out of the session.
    BracketArrowRight,
    /// Switch to another user.
    PersonArrows,
    /// A still camera.
    Camera,
    /// A numbered list.
    ListNumbers,
    /// A ticked box.
    SquareCheck,
    /// A paragraph of plain text.
    LetterT,
    /// Bold text.
    LetterB,
    /// Italic text.
    LetterI,
    /// Struck-through text.
    LetterSStrike,
    /// Code, written as angle brackets.
    BracketsAngle,
    /// A quotation, written as quotation marks.
    Quotes,
    /// A link, written as a chain.
    Link,
    /// Match case, a find option (Text Editor's find bar).
    LettersAa,
    /// Regular expression, a find option.
    Asterisk,
    /// Whole words, a find option.
    BracketsSquare,
    /// Save a document.
    FloppyDisk,
    /// Recent documents.
    ClockCcw,
    /// Shuffle the queue (Media).
    Shuffle,
    /// Repeat the queue (Media).
    Repeat,
    /// Repeat the item playing (Media).
    Repeat1,
    /// The mini player, picture in picture (Media).
    WindowInset,
    /// Up one level: an arrow pointing up (Files, the Browser header).
    ArrowUp,
    /// New tab: a rounded square with a plus (Browser).
    SquarePlus,
    /// Tab groups: a solid tab with one behind it (Browser).
    Tabs,
    /// Split view: a rounded square cut in two (Browser).
    WindowSplit,
    /// A private window: a domino mask (Browser).
    Mask,
    /// Reader view: an open book (Browser).
    BookOpen,
    /// Translate: the characters 文A (Browser).
    Translate,
    /// Find in page: a page with a magnifier over its corner (Browser).
    FileMagnifier,
    /// Reading list: glasses (Browser).
    Glasses,
    /// Bookmark this page: a bookmark with a plus (Browser).
    BookmarkPlus,
    /// Downloads: an arrow onto a line (Browser).
    ArrowDownLine,
    /// A finished download: a tick onto the same line (Browser).
    CheckLine,
    /// Sync: two arrows chasing round (Browser).
    ArrowsCw,
    /// Not secure: the padlock with its shackle open (Browser).
    PadlockOpen,
    /// Certificate not valid: the padlock with a cross (Browser).
    PadlockCross,
    /// Trackers blocked: a shield with a tick (Browser).
    ShieldCheck,
    /// Cookies and site data (Browser).
    Cookie,
    /// Location permission: a map pin (Browser).
    MapPin,
    /// Autoplay blocked: play with a slash (Browser).
    PlaySlash,
    /// Flexible space in a customisable toolbar (Browser).
    ArrowLeftRightBars,
    /// Space in a customisable toolbar (Browser).
    BracketBottom,
    /// Separator in a customisable toolbar (Browser).
    LineVertical,
    /// This tab crashed: a sad face on a tile (Browser).
    SquareFaceSad,
    /// Not responding: an hourglass (Browser).
    Hourglass,
    /// Any icon of the chat fork at any weight, as the fork draws it (feature `chat-icons`).
    #[cfg(feature = "chat-icons")]
    Chat(crate::chat_icons::ChatIcon, crate::chat_icons::ChatWeight),
}

impl Icon {
    /// Every icon, in declaration order.
    pub const ALL: [Icon; 202] = [
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::WindowSidebar,
        Self::Laptop,
        Self::Gear,
        Self::Monitor,
        Self::Cross,
        Self::Minus,
        Self::Square,
        Self::SquaresOverlapLeft,
        Self::Palette,
        Self::MonitorDock,
        Self::Magnifier,
        Self::Funnel,
        Self::Wifi,
        Self::Bluetooth,
        Self::SpeakerWave2,
        Self::Battery100,
        Self::Keyboard,
        Self::Mouse,
        Self::PersonCircle,
        Self::Clock,
        Self::People,
        Self::CircleInfo,
        Self::ArrowCw,
        Self::Trash,
        Self::Check,
        Self::Plus,
        Self::ArrowsLeftRight,
        Self::ChevronDown,
        Self::ChevronUp,
        Self::CircleCross,
        Self::TriangleExclamation,
        Self::SquaresOverlap,
        Self::Folder,
        Self::FolderOpen,
        Self::LinesThree,
        Self::DotsThree,
        Self::Cube,
        Self::DotsNine,
        Self::Headphones,
        Self::SpeakerBox,
        Self::Gamepad,
        Self::Phone,
        Self::Pause,
        Self::CursorClick,
        Self::Wifi0,
        Self::Wifi1,
        Self::Wifi2,
        Self::Padlock,
        Self::Network,
        Self::Shield,
        Self::Airplane,
        Self::Router,
        Self::WifiSlash,
        Self::WifiCross,
        Self::NetworkCross,
        Self::Microphone,
        Self::SpeakerCross,
        Self::Speaker,
        Self::SpeakerWave1,
        Self::Webcam,
        Self::Sun,
        Self::MoonStars,
        Self::Moon,
        Self::Leaf,
        Self::Gauge,
        Self::Bolt,
        Self::Power,
        Self::Battery0,
        Self::Battery10,
        Self::Battery20,
        Self::Battery30,
        Self::Battery40,
        Self::Battery50,
        Self::Battery60,
        Self::Battery70,
        Self::Battery80,
        Self::Battery90,
        Self::Eye,
        Self::EyeSlash,
        Self::SquarePencil,
        Self::Pencil,
        Self::IdCard,
        Self::StarFilled,
        Self::Star,
        Self::Play,
        Self::Stop,
        Self::CornersOut,
        Self::CornersIn,
        Self::MagnifierPlus,
        Self::MagnifierMinus,
        Self::FrameCorners,
        Self::ArrowCcw,
        Self::Crop,
        Self::Subtitles,
        Self::Key,
        Self::SquareArrowUp,
        Self::FolderArrowDown,
        Self::HandTap,
        Self::Sliders,
        Self::ListBullets,
        Self::Usb,
        Self::Printer,
        Self::Tablet,
        Self::Server,
        Self::Person,
        Self::Arrowhead,
        Self::Envelope,
        Self::MusicNotes,
        Self::Calculator,
        Self::Globe,
        Self::Calendar,
        Self::CircleQuestion,
        Self::SquaresFour,
        Self::Window,
        Self::BellSlash,
        Self::CoffeeCup,
        Self::Bookmark,
        Self::PuzzlePiece,
        Self::VideoCamera,
        Self::Image,
        Self::House,
        Self::FolderFileText,
        Self::FolderMusicNote,
        Self::FolderImage,
        Self::FolderPlay,
        Self::HardDrive,
        Self::FileText,
        Self::FileAudio,
        Self::Package,
        Self::FolderPlus,
        Self::FolderDashed,
        Self::Eject,
        Self::Disc,
        Self::FolderFile,
        Self::FolderPerson,
        Self::FolderMonitor,
        Self::SquareArrowUpRight,
        Self::FileDoc,
        Self::DotsSix,
        Self::FilePdf,
        Self::Table,
        Self::Easel,
        Self::SquareLettersAa,
        Self::FileCode,
        Self::File,
        Self::CircleArrowUpRight,
        Self::Record,
        Self::SkipBack,
        Self::SkipForward,
        Self::Rewind,
        Self::FastForward,
        Self::MicrophoneSlash,
        Self::ArrowsUpDown,
        Self::Devices,
        Self::MonitorArrowUp,
        Self::BracketArrowRight,
        Self::PersonArrows,
        Self::Camera,
        Self::ListNumbers,
        Self::SquareCheck,
        Self::LetterT,
        Self::LetterB,
        Self::LetterI,
        Self::LetterSStrike,
        Self::BracketsAngle,
        Self::Quotes,
        Self::Link,
        Self::LettersAa,
        Self::Asterisk,
        Self::BracketsSquare,
        Self::FloppyDisk,
        Self::ClockCcw,
        Self::Shuffle,
        Self::Repeat,
        Self::Repeat1,
        Self::WindowInset,
        Self::ArrowUp,
        Self::SquarePlus,
        Self::Tabs,
        Self::WindowSplit,
        Self::Mask,
        Self::BookOpen,
        Self::Translate,
        Self::FileMagnifier,
        Self::Glasses,
        Self::BookmarkPlus,
        Self::ArrowDownLine,
        Self::CheckLine,
        Self::ArrowsCw,
        Self::PadlockOpen,
        Self::PadlockCross,
        Self::ShieldCheck,
        Self::Cookie,
        Self::MapPin,
        Self::PlaySlash,
        Self::ArrowLeftRightBars,
        Self::BracketBottom,
        Self::LineVertical,
        Self::SquareFaceSad,
        Self::Hourglass,
    ];

    /// The embedded SVG source for this icon.
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::ChevronLeft => include_bytes!("../icons/chevron-left-symbolic.svg"),
            Self::ChevronRight => include_bytes!("../icons/chevron-right-symbolic.svg"),
            Self::WindowSidebar => include_bytes!("../icons/window-sidebar-symbolic.svg"),
            Self::Laptop => include_bytes!("../icons/laptop-symbolic.svg"),
            Self::Gear => include_bytes!("../icons/gear-symbolic.svg"),
            Self::Monitor => include_bytes!("../icons/monitor-symbolic.svg"),
            Self::Cross => include_bytes!("../icons/cross-symbolic.svg"),
            Self::Minus => include_bytes!("../icons/minus-symbolic.svg"),
            Self::Square => include_bytes!("../icons/square-symbolic.svg"),
            Self::SquaresOverlapLeft => {
                include_bytes!("../icons/squares-overlap-left-symbolic.svg")
            }
            Self::Palette => include_bytes!("../icons/palette-symbolic.svg"),
            Self::MonitorDock => include_bytes!("../icons/monitor-dock-symbolic.svg"),
            Self::Magnifier => include_bytes!("../icons/magnifier-symbolic.svg"),
            Self::Funnel => include_bytes!("../icons/funnel-symbolic.svg"),
            Self::Wifi => include_bytes!("../icons/wifi-symbolic.svg"),
            Self::Bluetooth => include_bytes!("../icons/bluetooth-symbolic.svg"),
            Self::SpeakerWave2 => include_bytes!("../icons/speaker-wave-2-symbolic.svg"),
            Self::Battery100 => include_bytes!("../icons/battery-100-symbolic.svg"),
            Self::Keyboard => include_bytes!("../icons/keyboard-symbolic.svg"),
            Self::Mouse => include_bytes!("../icons/mouse-symbolic.svg"),
            Self::PersonCircle => {
                include_bytes!("../icons/person-circle-symbolic.svg")
            }
            Self::Clock => include_bytes!("../icons/clock-symbolic.svg"),
            Self::People => include_bytes!("../icons/people-symbolic.svg"),
            Self::CircleInfo => include_bytes!("../icons/circle-info-symbolic.svg"),
            Self::ArrowCw => include_bytes!("../icons/arrow-cw-symbolic.svg"),
            Self::Trash => include_bytes!("../icons/trash-symbolic.svg"),
            Self::Check => include_bytes!("../icons/check-symbolic.svg"),
            Self::Plus => include_bytes!("../icons/plus-symbolic.svg"),
            Self::ArrowsLeftRight => include_bytes!("../icons/arrows-left-right-symbolic.svg"),
            Self::ChevronDown => include_bytes!("../icons/chevron-down-symbolic.svg"),
            Self::ChevronUp => include_bytes!("../icons/chevron-up-symbolic.svg"),
            Self::CircleCross => include_bytes!("../icons/circle-cross-symbolic.svg"),
            Self::TriangleExclamation => {
                include_bytes!("../icons/triangle-exclamation-symbolic.svg")
            }
            Self::SquaresOverlap => include_bytes!("../icons/squares-overlap-symbolic.svg"),
            Self::Folder => include_bytes!("../icons/folder-symbolic.svg"),
            Self::FolderOpen => include_bytes!("../icons/folder-open-symbolic.svg"),
            Self::LinesThree => include_bytes!("../icons/lines-three-symbolic.svg"),
            Self::DotsThree => include_bytes!("../icons/dots-three-symbolic.svg"),
            Self::Cube => include_bytes!("../icons/cube-symbolic.svg"),
            Self::DotsNine => include_bytes!("../icons/dots-nine-symbolic.svg"),
            Self::Headphones => include_bytes!("../icons/headphones-symbolic.svg"),
            Self::SpeakerBox => include_bytes!("../icons/speaker-box-symbolic.svg"),
            Self::Gamepad => include_bytes!("../icons/gamepad-symbolic.svg"),
            Self::Phone => include_bytes!("../icons/phone-symbolic.svg"),
            Self::Pause => include_bytes!("../icons/pause-symbolic.svg"),
            // The element picker, as GTK's own inspector uses.
            Self::CursorClick => include_bytes!("../icons/cursor-click-symbolic.svg"),
            Self::Wifi0 => {
                include_bytes!("../icons/wifi-0-symbolic.svg")
            }
            Self::Wifi1 => {
                include_bytes!("../icons/wifi-1-symbolic.svg")
            }
            Self::Wifi2 => {
                include_bytes!("../icons/wifi-2-symbolic.svg")
            }
            Self::Padlock => {
                include_bytes!("../icons/padlock-symbolic.svg")
            }
            Self::Network => include_bytes!("../icons/network-symbolic.svg"),
            Self::Shield => include_bytes!("../icons/shield-symbolic.svg"),
            Self::Airplane => include_bytes!("../icons/airplane-symbolic.svg"),
            Self::Router => {
                include_bytes!("../icons/router-symbolic.svg")
            }
            Self::WifiSlash => {
                include_bytes!("../icons/wifi-slash-symbolic.svg")
            }
            Self::WifiCross => include_bytes!("../icons/wifi-cross-symbolic.svg"),
            Self::NetworkCross => include_bytes!("../icons/network-cross-symbolic.svg"),
            Self::Microphone => {
                include_bytes!("../icons/microphone-symbolic.svg")
            }
            Self::SpeakerCross => include_bytes!("../icons/speaker-cross-symbolic.svg"),
            Self::Speaker => include_bytes!("../icons/speaker-symbolic.svg"),
            Self::SpeakerWave1 => include_bytes!("../icons/speaker-wave-1-symbolic.svg"),
            Self::Webcam => include_bytes!("../icons/webcam-symbolic.svg"),
            Self::Sun => include_bytes!("../icons/sun-symbolic.svg"),
            Self::MoonStars => include_bytes!("../icons/moon-stars-symbolic.svg"),
            Self::Moon => include_bytes!("../icons/moon-symbolic.svg"),
            Self::Leaf => {
                include_bytes!("../icons/leaf-symbolic.svg")
            }
            Self::Gauge => {
                include_bytes!("../icons/gauge-symbolic.svg")
            }
            Self::Bolt => {
                include_bytes!("../icons/bolt-symbolic.svg")
            }
            Self::Power => include_bytes!("../icons/power-symbolic.svg"),
            Self::Battery0 => include_bytes!("../icons/battery-0-symbolic.svg"),
            Self::Battery10 => include_bytes!("../icons/battery-10-symbolic.svg"),
            Self::Battery20 => include_bytes!("../icons/battery-20-symbolic.svg"),
            Self::Battery30 => include_bytes!("../icons/battery-30-symbolic.svg"),
            Self::Battery40 => include_bytes!("../icons/battery-40-symbolic.svg"),
            Self::Battery50 => include_bytes!("../icons/battery-50-symbolic.svg"),
            Self::Battery60 => include_bytes!("../icons/battery-60-symbolic.svg"),
            Self::Battery70 => include_bytes!("../icons/battery-70-symbolic.svg"),
            Self::Battery80 => include_bytes!("../icons/battery-80-symbolic.svg"),
            Self::Battery90 => include_bytes!("../icons/battery-90-symbolic.svg"),
            Self::Eye => include_bytes!("../icons/eye-symbolic.svg"),
            Self::EyeSlash => include_bytes!("../icons/eye-slash-symbolic.svg"),
            Self::SquarePencil => include_bytes!("../icons/square-pencil-symbolic.svg"),
            Self::Pencil => include_bytes!("../icons/pencil-symbolic.svg"),
            Self::IdCard => include_bytes!("../icons/id-card-symbolic.svg"),
            Self::StarFilled => include_bytes!("../icons/star-filled-symbolic.svg"),
            Self::Star => include_bytes!("../icons/star-symbolic.svg"),
            Self::Play => {
                include_bytes!("../icons/play-symbolic.svg")
            }
            Self::Stop => include_bytes!("../icons/stop-symbolic.svg"),
            Self::CornersOut => include_bytes!("../icons/corners-out-symbolic.svg"),
            Self::CornersIn => include_bytes!("../icons/corners-in-symbolic.svg"),
            Self::MagnifierPlus => include_bytes!("../icons/magnifier-plus-symbolic.svg"),
            Self::MagnifierMinus => include_bytes!("../icons/magnifier-minus-symbolic.svg"),
            Self::FrameCorners => include_bytes!("../icons/frame-corners-symbolic.svg"),
            Self::ArrowCcw => include_bytes!("../icons/arrow-ccw-symbolic.svg"),
            Self::Crop => include_bytes!("../icons/crop-symbolic.svg"),
            Self::Subtitles => {
                include_bytes!("../icons/subtitles-symbolic.svg")
            }
            Self::Key => include_bytes!("../icons/key-symbolic.svg"),
            Self::SquareArrowUp => include_bytes!("../icons/square-arrow-up-symbolic.svg"),
            Self::FolderArrowDown => include_bytes!("../icons/folder-arrow-down-symbolic.svg"),
            Self::HandTap => include_bytes!("../icons/hand-tap-symbolic.svg"),
            Self::Sliders => include_bytes!("../icons/sliders-symbolic.svg"),
            Self::ListBullets => include_bytes!("../icons/list-bullets-symbolic.svg"),
            Self::Usb => include_bytes!("../icons/usb-symbolic.svg"),
            Self::Printer => include_bytes!("../icons/printer-symbolic.svg"),
            Self::Tablet => include_bytes!("../icons/tablet-symbolic.svg"),
            Self::Server => include_bytes!("../icons/server-symbolic.svg"),
            Self::Person => include_bytes!("../icons/person-symbolic.svg"),
            Self::Arrowhead => include_bytes!("../icons/arrowhead-symbolic.svg"),
            Self::Envelope => include_bytes!("../icons/envelope-symbolic.svg"),
            Self::MusicNotes => {
                include_bytes!("../icons/music-notes-symbolic.svg")
            }
            Self::Calculator => {
                include_bytes!("../icons/calculator-symbolic.svg")
            }
            Self::Globe => include_bytes!("../icons/globe-symbolic.svg"),
            Self::Calendar => include_bytes!("../icons/calendar-symbolic.svg"),
            Self::CircleQuestion => include_bytes!("../icons/circle-question-symbolic.svg"),
            Self::SquaresFour => include_bytes!("../icons/squares-four-symbolic.svg"),
            Self::Window => include_bytes!("../icons/window-symbolic.svg"),
            Self::BellSlash => {
                include_bytes!("../icons/bell-slash-symbolic.svg")
            }
            Self::CoffeeCup => include_bytes!("../icons/coffee-cup-symbolic.svg"),
            Self::Bookmark => include_bytes!("../icons/bookmark-symbolic.svg"),
            Self::PuzzlePiece => include_bytes!("../icons/puzzle-piece-symbolic.svg"),
            Self::VideoCamera => include_bytes!("../icons/video-camera-symbolic.svg"),
            Self::Image => include_bytes!("../icons/image-symbolic.svg"),
            Self::House => include_bytes!("../icons/house-symbolic.svg"),
            Self::FolderFileText => include_bytes!("../icons/folder-file-text-symbolic.svg"),
            Self::FolderMusicNote => include_bytes!("../icons/folder-music-note-symbolic.svg"),
            Self::FolderImage => include_bytes!("../icons/folder-image-symbolic.svg"),
            Self::FolderPlay => include_bytes!("../icons/folder-play-symbolic.svg"),
            Self::HardDrive => include_bytes!("../icons/hard-drive-symbolic.svg"),
            Self::FileText => include_bytes!("../icons/file-text-symbolic.svg"),
            Self::FileAudio => include_bytes!("../icons/file-audio-symbolic.svg"),
            Self::Package => include_bytes!("../icons/package-symbolic.svg"),
            Self::FolderPlus => include_bytes!("../icons/folder-plus-symbolic.svg"),
            Self::FolderDashed => include_bytes!("../icons/folder-dashed-symbolic.svg"),
            Self::Eject => include_bytes!("../icons/eject-symbolic.svg"),
            Self::Disc => include_bytes!("../icons/disc-symbolic.svg"),
            Self::FolderFile => include_bytes!("../icons/folder-file-symbolic.svg"),
            Self::FolderPerson => include_bytes!("../icons/folder-person-symbolic.svg"),
            Self::FolderMonitor => include_bytes!("../icons/folder-monitor-symbolic.svg"),
            Self::SquareArrowUpRight => {
                include_bytes!("../icons/square-arrow-up-right-symbolic.svg")
            }
            Self::FileDoc => {
                include_bytes!("../icons/file-doc-symbolic.svg")
            }
            Self::DotsSix => include_bytes!("../icons/dots-six-symbolic.svg"),
            Self::FilePdf => include_bytes!("../icons/file-pdf-symbolic.svg"),
            Self::Table => include_bytes!("../icons/table-symbolic.svg"),
            Self::Easel => include_bytes!("../icons/easel-symbolic.svg"),
            Self::SquareLettersAa => include_bytes!("../icons/square-letters-aa-symbolic.svg"),
            Self::FileCode => include_bytes!("../icons/file-code-symbolic.svg"),
            Self::File => {
                include_bytes!("../icons/file-symbolic.svg")
            }
            Self::CircleArrowUpRight => {
                include_bytes!("../icons/circle-arrow-up-right-symbolic.svg")
            }
            Self::Record => include_bytes!("../icons/record-symbolic.svg"),
            Self::SkipBack => include_bytes!("../icons/skip-back-symbolic.svg"),
            Self::SkipForward => include_bytes!("../icons/skip-forward-symbolic.svg"),
            Self::Rewind => include_bytes!("../icons/rewind-symbolic.svg"),
            Self::FastForward => include_bytes!("../icons/fast-forward-symbolic.svg"),
            Self::MicrophoneSlash => {
                include_bytes!("../icons/microphone-slash-symbolic.svg")
            }
            Self::ArrowsUpDown => {
                include_bytes!("../icons/arrows-up-down-symbolic.svg")
            }
            Self::Devices => include_bytes!("../icons/devices-symbolic.svg"),
            Self::MonitorArrowUp => include_bytes!("../icons/monitor-arrow-up-symbolic.svg"),
            Self::BracketArrowRight => include_bytes!("../icons/bracket-arrow-right-symbolic.svg"),
            Self::PersonArrows => include_bytes!("../icons/person-arrows-symbolic.svg"),
            Self::Camera => include_bytes!("../icons/camera-symbolic.svg"),
            Self::ListNumbers => include_bytes!("../icons/list-numbers-symbolic.svg"),
            Self::SquareCheck => include_bytes!("../icons/square-check-symbolic.svg"),
            Self::LetterT => {
                include_bytes!("../icons/letter-t-symbolic.svg")
            }
            Self::LetterB => include_bytes!("../icons/letter-b-symbolic.svg"),
            Self::LetterI => include_bytes!("../icons/letter-i-symbolic.svg"),
            Self::LetterSStrike => {
                include_bytes!("../icons/letter-s-strike-symbolic.svg")
            }
            Self::BracketsAngle => include_bytes!("../icons/brackets-angle-symbolic.svg"),
            Self::Quotes => include_bytes!("../icons/quotes-symbolic.svg"),
            Self::Link => include_bytes!("../icons/link-symbolic.svg"),
            Self::LettersAa => include_bytes!("../icons/letters-aa-symbolic.svg"),
            Self::Asterisk => include_bytes!("../icons/asterisk-symbolic.svg"),
            Self::BracketsSquare => include_bytes!("../icons/brackets-square-symbolic.svg"),
            Self::FloppyDisk => include_bytes!("../icons/floppy-disk-symbolic.svg"),
            Self::ClockCcw => {
                include_bytes!("../icons/clock-ccw-symbolic.svg")
            }
            Self::Shuffle => {
                include_bytes!("../icons/shuffle-symbolic.svg")
            }
            Self::Repeat => {
                include_bytes!("../icons/repeat-symbolic.svg")
            }
            Self::Repeat1 => {
                include_bytes!("../icons/repeat-1-symbolic.svg")
            }
            Self::WindowInset => include_bytes!("../icons/window-inset-symbolic.svg"),
            Self::ArrowUp => include_bytes!("../icons/arrow-up-symbolic.svg"),
            Self::SquarePlus => include_bytes!("../icons/square-plus-symbolic.svg"),
            Self::Tabs => include_bytes!("../icons/tabs-symbolic.svg"),
            Self::WindowSplit => include_bytes!("../icons/window-split-symbolic.svg"),
            Self::Mask => include_bytes!("../icons/mask-symbolic.svg"),
            Self::BookOpen => include_bytes!("../icons/book-open-symbolic.svg"),
            Self::Translate => include_bytes!("../icons/translate-symbolic.svg"),
            Self::FileMagnifier => include_bytes!("../icons/file-magnifier-symbolic.svg"),
            Self::Glasses => include_bytes!("../icons/glasses-symbolic.svg"),
            Self::BookmarkPlus => include_bytes!("../icons/bookmark-plus-symbolic.svg"),
            Self::ArrowDownLine => include_bytes!("../icons/arrow-down-line-symbolic.svg"),
            Self::CheckLine => include_bytes!("../icons/check-line-symbolic.svg"),
            Self::ArrowsCw => include_bytes!("../icons/arrows-cw-symbolic.svg"),
            Self::PadlockOpen => include_bytes!("../icons/padlock-open-symbolic.svg"),
            Self::PadlockCross => include_bytes!("../icons/padlock-cross-symbolic.svg"),
            Self::ShieldCheck => include_bytes!("../icons/shield-check-symbolic.svg"),
            Self::Cookie => include_bytes!("../icons/cookie-symbolic.svg"),
            Self::MapPin => include_bytes!("../icons/map-pin-symbolic.svg"),
            Self::PlaySlash => include_bytes!("../icons/play-slash-symbolic.svg"),
            Self::ArrowLeftRightBars => {
                include_bytes!("../icons/arrow-left-right-bars-symbolic.svg")
            }
            Self::BracketBottom => include_bytes!("../icons/bracket-bottom-symbolic.svg"),
            Self::LineVertical => include_bytes!("../icons/line-vertical-symbolic.svg"),
            Self::SquareFaceSad => include_bytes!("../icons/square-face-sad-symbolic.svg"),
            Self::Hourglass => include_bytes!("../icons/hourglass-symbolic.svg"),
            #[cfg(feature = "chat-icons")]
            // A weight the fork lacks is drawn in the regular weight, which every fork icon has.
            Self::Chat(icon, weight) => icon
                .bytes(weight)
                .or_else(|| icon.bytes(crate::chat_icons::ChatWeight::Regular))
                .expect("every fork icon has a regular weight"),
        }
    }
}

/// The image source of `icon`. Its id is the address of the embedded file, so
/// building one hashes a pointer and not the SVG: a page draws hundreds.
fn source(icon: Icon) -> ImageSource {
    let bytes = icon.bytes();
    ImageSource::from((bytes.as_ptr() as usize, bytes))
}

/// The viewer behind every icon.
///
/// Rendering an icon only looks its raster up in the asset cache (keyed by
/// the icon, the size in device pixels and the colour): the first draw of a
/// combination rasterises it on a worker, and until it lands the viewer is an
/// empty box of the same size, so nothing moves. `render` never rasterises on
/// the UI thread. The colour is set as both `fill` and `color` so the key does
/// not change when the viewer learns its inherited text colour after the first
/// draw, which would rasterise every icon twice.
pub(crate) fn glyph(icon: Icon, edge: f32, color: Color) -> SvgViewer {
    let viewer = off_thread(
        SvgViewer::new(source(icon))
            .width(Size::px(edge))
            .height(Size::px(edge))
            .fill(color),
        color,
    );
    if icon.mirrors_in_rtl() && crate::controls::direction::use_direction().is_rtl() {
        viewer.scale((-1.0, 1.0))
    } else {
        viewer
    }
}

/// Makes `viewer` rasterise on a worker, as an empty box of its size until it
/// lands, keyed on `color` from the first draw (as `fill` and `currentColor`)
/// so it does not rasterise a second time when it learns its inherited colour.
pub(crate) fn off_thread(viewer: SvgViewer, color: Color) -> SvgViewer {
    viewer
        .color(color)
        .async_rasterization(true)
        .show_loader(false)
}

/// Draws `icon` as a square of `size` filled with `color`. Pass a token
/// colour: `Paint::Muted.color(&tokens)`.
pub fn icon(icon: Icon, size: crate::style::IconSize, color: Color) -> SvgViewer {
    glyph(icon, size.px(), color)
}

/// [`icon`] with its size scaled by `scale` and rounded to whole pixels, so a
/// zooming view keeps its glyph edges sharp. The size is never below 1 px.
pub fn icon_scaled(
    icon: Icon,
    size: crate::style::IconSize,
    scale: f32,
    color: Color,
) -> SvgViewer {
    glyph(icon, (size.px() * scale).round().max(1.0), color)
}

/// Rasterises `icon` at `size` in `color` on a worker, into the cache that
/// [`icon`] reads, so the first frame that draws it finds it there. Call from
/// a hook at start-up for the icons the first frame shows (the sidebar and
/// the header); anything else rasterises when it first appears. A combination
/// that is already cached or on its way costs nothing.
pub fn warm(icon: Icon, size: crate::style::IconSize, color: Color) {
    warm_svg(source(icon), size.px(), color);
}

impl Icon {
    /// Whether the icon is drawn flipped left to right when the interface reads
    /// right to left: what it draws has a direction in the reading order (a
    /// chevron or arrow, the side a sidebar toggle's panel is on, a list's
    /// bullets, a paper plane being sent). The set is `#mirror_rtl` in
    /// `scripts/icons/labels.tsv`, with the reasons in `design/rtl.md`, and a
    /// test keeps the two equal. [`icon`] and [`icon_scaled`] apply it.
    pub const fn mirrors_in_rtl(self) -> bool {
        matches!(
            self,
            Self::ChevronLeft
                | Self::ChevronRight
                | Self::WindowSidebar
                | Self::SquareArrowUp
                | Self::ListBullets
                | Self::CircleQuestion
                | Self::CircleArrowUpRight
                | Self::BracketArrowRight
        )
    }

    /// The battery icon filled to `percent` (0 to 100, more is 100), rounded to
    /// the nearest ten: one of the `battery-level-N` files, which draw the
    /// charge as the filled part of the body and the rest dimmed.
    pub const fn battery_level(percent: u8) -> Self {
        match (percent.saturating_add(5) / 10) * 10 {
            0 => Self::Battery0,
            10 => Self::Battery10,
            20 => Self::Battery20,
            30 => Self::Battery30,
            40 => Self::Battery40,
            50 => Self::Battery50,
            60 => Self::Battery60,
            70 => Self::Battery70,
            80 => Self::Battery80,
            90 => Self::Battery90,
            _ => Self::Battery100,
        }
    }
}

include!("icon_aliases.rs");

#[cfg(test)]
mod tests {
    use super::*;

    /// The files `#mirror_rtl` names in `scripts/icons/labels.tsv`.
    fn labelled() -> Vec<&'static str> {
        include_str!("../../../scripts/icons/labels.tsv")
            .lines()
            .find_map(|line| line.strip_prefix("#mirror_rtl:"))
            .expect("labels.tsv has a #mirror_rtl directive")
            .split_whitespace()
            .collect()
    }

    #[test]
    fn mirrored_icons_are_the_ones_labels_tsv_names() {
        let labelled = labelled();
        // Two files may hold the same drawing, so an icon is every file whose
        // bytes it is.
        let files_of = |icon: Icon| -> Vec<String> {
            let bytes = icon.bytes();
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/icons"))
                .expect("icons/ is readable")
                .filter_map(Result::ok)
                .filter(|entry| std::fs::read(entry.path()).is_ok_and(|file| file == bytes))
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter_map(|name| name.strip_suffix("-symbolic.svg").map(str::to_string))
                .collect()
        };
        for icon in Icon::ALL {
            let files = files_of(icon);
            assert!(!files.is_empty(), "{icon:?} is a file in icons/");
            assert_eq!(
                icon.mirrors_in_rtl(),
                files.iter().any(|file| labelled.contains(&file.as_str())),
                "{icon:?} ({files:?}): Icon::mirrors_in_rtl and `#mirror_rtl` in labels.tsv disagree"
            );
        }
        for file in labelled {
            assert!(
                Icon::ALL
                    .iter()
                    .any(|icon| files_of(*icon).iter().any(|name| name == file)),
                "`#mirror_rtl` names {file}, which no Icon draws"
            );
        }
    }

    #[test]
    fn battery_level_picks_the_nearest_ten_and_never_goes_past_full() {
        assert_eq!(Icon::battery_level(0), Icon::Battery0);
        assert_eq!(Icon::battery_level(4), Icon::Battery0);
        assert_eq!(Icon::battery_level(5), Icon::Battery10);
        assert_eq!(Icon::battery_level(49), Icon::Battery50);
        assert_eq!(Icon::battery_level(85), Icon::Battery90);
        assert_eq!(Icon::battery_level(100), Icon::Battery100);
        assert_eq!(Icon::battery_level(255), Icon::Battery100);
    }

    #[test]
    fn freedesktop_names_find_the_icon_that_draws_them() {
        assert_eq!(Icon::from_freedesktop("wifi"), Some(Icon::Wifi));
        assert_eq!(
            Icon::from_freedesktop("network-wireless-signal-good-symbolic"),
            Some(Icon::Wifi)
        );
        assert_eq!(
            Icon::from_freedesktop("audio-headset"),
            Some(Icon::Headphones)
        );
        assert_eq!(
            Icon::from_freedesktop("camera-video"),
            Some(Icon::VideoCamera)
        );
        assert_eq!(
            Icon::from_freedesktop("power-profile-power-saver"),
            Some(Icon::Leaf)
        );
        assert_eq!(Icon::from_freedesktop("no-such-icon"), None);
    }
}
