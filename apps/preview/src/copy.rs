//! Every word Preview shows, in one place: menu items, tooltips, the empty
//! state, toasts and the shortcut list. Whole sentences with their
//! placeables, never built from pieces, so each moves into a catalogue as it
//! is.

pub const OPEN_TOOLTIP: &str = "Open (Ctrl+O)";
pub const INFO_TOOLTIP: &str = "Info (I)";
pub const PAGES_TOOLTIP: &str = "Pages (F9)";
pub const TURN_LEFT_TOOLTIP: &str = "Turn left (Shift+R)";
pub const TURN_RIGHT_TOOLTIP: &str = "Turn right (R)";
pub const CROP_TOOLTIP: &str = "Crop (C)";
pub const MAIN_MENU: &str = "Main menu";

pub const MENU_OPEN: &str = "Open…";
pub const MENU_OPEN_WITH: &str = "Open with";
pub const MENU_SHOW_IN_FILES: &str = "Show in Files";
pub const MENU_COPY_PICTURE: &str = "Copy picture";
pub const MENU_SAVE_COPY: &str = "Save a copy…";
pub const MENU_CROP: &str = "Crop…";
pub const MENU_TURN_LEFT: &str = "Turn left";
pub const MENU_TURN_RIGHT: &str = "Turn right";
pub const MENU_FLIP_HORIZONTALLY: &str = "Flip horizontally";
pub const MENU_FLIP_VERTICALLY: &str = "Flip vertically";
pub const MENU_FIT: &str = "Fit to window";
pub const MENU_ACTUAL_SIZE: &str = "Actual size";
pub const MENU_SLIDESHOW: &str = "Slideshow";
pub const MENU_STOP_SLIDESHOW: &str = "Stop slideshow";
pub const MENU_FULL_SCREEN: &str = "Full screen";
pub const MENU_LEAVE_FULL_SCREEN: &str = "Leave full screen";
pub const MENU_TRASH: &str = "Move to Trash";
pub const MENU_UNDO: &str = "Undo";
pub const MENU_SHORTCUTS: &str = "Keyboard shortcuts";

pub const EMPTY_TITLE: &str = "No file open";
pub const EMPTY_LINE: &str = "Drop a picture or document here.";
pub const EMPTY_ACTION: &str = "Open file";
pub const DROP_TITLE: &str = "Drop to open";

pub const OPEN_REQUEST_TITLE: &str = "Open";
pub const OPEN_FILTER: &str = "Pictures and documents";
pub const SAVE_REQUEST_TITLE: &str = "Save a copy";

pub const NOTHING_TO_SHOW: &str = "Nothing to show";

pub const COPIED: &str = "Picture copied";
pub const COPY_FAILED: &str = "Could not copy the picture. Install wl-clipboard or xclip, then try again.";
pub const SAVE_FAILED: &str =
    "Could not save the copy. Check that the folder can be written to, then try again.";
pub const CROP_AREA: &str = "Crop area";
pub const CROP_SAVE: &str = "Save a copy";
pub const CANCEL: &str = "Cancel";
pub const CROP_FAILED: &str = "Could not open the picture to crop it. Try another picture.";
pub const UNDO_COPY_FAILED: &str =
    "Could not remove the copy. Move it to the Trash from Files.";
pub const UNDO: &str = "Undo";
pub const UNDO_FAILED: &str = "Could not put the file back. Open the Trash in Files to restore it.";

/// "3 of 24".
pub fn position(at: usize, of: usize) -> String {
    format!("{at} of {of}")
}

/// A name the person's file has, in the typographic quotes the voice uses.
pub fn quoted(name: &str) -> String {
    format!("“{name}”")
}

pub fn moved_to_trash(name: &str) -> String {
    format!("{} moved to the Trash", quoted(name))
}

pub fn trash_failed(name: &str) -> String {
    format!(
        "Could not move {} to the Trash. Check that you can change its folder.",
        quoted(name)
    )
}

pub fn saved_copy(name: &str) -> String {
    format!("Copy saved as {}", quoted(name))
}

pub fn saved_crop(name: &str) -> String {
    format!("Cropped copy saved as {}", quoted(name))
}

pub fn app_failed(app: &str) -> String {
    format!(
        "Could not start {}. Check that it is still installed, then try again.",
        quoted(app)
    )
}

pub fn show_in_files_failed() -> &'static str {
    "Could not show the file. Open its folder in Files."
}

/// The picture a screen reader names for the stage.
pub fn picture_label(name: &str) -> String {
    format!("Picture: {name}")
}

/// What the shortcuts dialog says.
pub const SHORTCUTS: &str = "\
Ctrl+O  Open\n\
Left, Right  Previous and next file in the folder\n\
Space  Play or pause an animated picture, page down in a document\n\
+, -, 0, 1  Zoom in, zoom out, fit the window, actual size\n\
R, Shift+R  Turn right, turn left\n\
H, V  Flip horizontally, vertically\n\
Ctrl and wheel  Zoom at the pointer\n\
Shift and an arrow  Move a zoomed picture\n\
Ctrl+C  Copy the picture, or the selected text of a document\n\
Ctrl+Shift+S  Save a copy of a picture\n\
C  Crop a picture and save the result as a copy\n\
Delete  Move a picture to the Trash\n\
Ctrl+Z  Put the last file moved to the Trash back, or remove the last cropped copy\n\
S  Start or stop a slideshow of the folder\n\
I  Show or hide the information\n\
F9  Show or hide the pages of a document\n\
Ctrl+F  Find in a document\n\
F, F11  Full screen\n\
Escape  Leave full screen or the slideshow";
