//! Every word Preview shows, in one place: menu items, tooltips, the empty
//! state, toasts and the shortcut list. Each one is a `tr!` over the
//! `preview-*` messages in `locale/en-US/preview.ftl`, so it is translated
//! as a whole sentence with its variables.

use bingux_ui::prelude::tr;

pub fn open_tooltip() -> String {
    tr!("preview-open-tooltip")
}
pub fn info_tooltip() -> String {
    tr!("preview-info-tooltip")
}
pub fn pages_tooltip() -> String {
    tr!("preview-pages-tooltip")
}
pub fn turn_left_tooltip() -> String {
    tr!("preview-turn-left-tooltip")
}
pub fn turn_right_tooltip() -> String {
    tr!("preview-turn-right-tooltip")
}
pub fn crop_tooltip() -> String {
    tr!("preview-crop-tooltip")
}
pub fn main_menu() -> String {
    tr!("preview-main-menu")
}

pub fn menu_open() -> String {
    tr!("preview-menu-open")
}
pub fn menu_open_with() -> String {
    tr!("preview-menu-open-with")
}
pub fn menu_show_in_files() -> String {
    tr!("preview-menu-show-in-files")
}
pub fn menu_copy_picture() -> String {
    tr!("preview-menu-copy-picture")
}
pub fn menu_save_copy() -> String {
    tr!("preview-menu-save-copy")
}
pub fn menu_crop() -> String {
    tr!("preview-menu-crop")
}
pub fn menu_turn_left() -> String {
    tr!("preview-menu-turn-left")
}
pub fn menu_turn_right() -> String {
    tr!("preview-menu-turn-right")
}
pub fn menu_flip_horizontally() -> String {
    tr!("preview-menu-flip-horizontally")
}
pub fn menu_flip_vertically() -> String {
    tr!("preview-menu-flip-vertically")
}
pub fn menu_fit() -> String {
    tr!("preview-menu-fit")
}
pub fn menu_actual_size() -> String {
    tr!("preview-menu-actual-size")
}
pub fn menu_slideshow() -> String {
    tr!("preview-menu-slideshow")
}
pub fn menu_stop_slideshow() -> String {
    tr!("preview-menu-stop-slideshow")
}
pub fn menu_full_screen() -> String {
    tr!("preview-menu-full-screen")
}
pub fn menu_leave_full_screen() -> String {
    tr!("preview-menu-leave-full-screen")
}
pub fn menu_trash() -> String {
    tr!("preview-menu-trash")
}
pub fn menu_undo() -> String {
    tr!("preview-menu-undo")
}
pub fn menu_shortcuts() -> String {
    tr!("preview-menu-shortcuts")
}

pub fn empty_title() -> String {
    tr!("preview-empty-title")
}
pub fn empty_line() -> String {
    tr!("preview-empty-line")
}
pub fn empty_action() -> String {
    tr!("preview-empty-action")
}
pub fn drop_title() -> String {
    tr!("preview-drop-title")
}

pub fn open_request_title() -> String {
    tr!("preview-open-request-title")
}
pub fn open_filter() -> String {
    tr!("preview-open-filter")
}
pub fn save_request_title() -> String {
    tr!("preview-save-request-title")
}

pub fn copied() -> String {
    tr!("preview-copied")
}
pub fn copy_failed() -> String {
    tr!("preview-copy-failed")
}
pub fn save_failed() -> String {
    tr!("preview-save-failed")
}
pub fn crop_area() -> String {
    tr!("preview-crop-area")
}
pub fn crop_save() -> String {
    tr!("preview-crop-save")
}
pub fn cancel() -> String {
    tr!("preview-cancel")
}
pub fn close() -> String {
    tr!("preview-close")
}
pub fn crop_failed() -> String {
    tr!("preview-crop-failed")
}
pub fn crop_undone() -> String {
    tr!("preview-crop-undone")
}
pub fn undo() -> String {
    tr!("preview-undo")
}
pub fn undo_copy_failed() -> String {
    tr!("preview-undo-copy-failed")
}
pub fn undo_failed() -> String {
    tr!("preview-undo-failed")
}

/// "3 of 24".
pub fn position(at: usize, of: usize) -> String {
    tr!("preview-position", at = at, of = of)
}

pub fn moved_to_trash(name: &str) -> String {
    tr!("preview-moved-to-trash", name = name)
}

pub fn trash_failed(name: &str) -> String {
    tr!("preview-trash-failed", name = name)
}

pub fn saved_copy(name: &str) -> String {
    tr!("preview-saved-copy", name = name)
}

pub fn saved_crop(name: &str) -> String {
    tr!("preview-saved-crop", name = name)
}

pub fn app_failed(app: &str) -> String {
    tr!("preview-app-failed", app = app)
}

pub fn show_in_files_failed() -> String {
    tr!("preview-show-in-files-failed")
}

/// The picture a screen reader names for the stage.
pub fn picture_label(name: &str) -> String {
    tr!("preview-picture-label", name = name)
}

/// What the shortcuts dialog says.
pub fn shortcuts() -> String {
    tr!("preview-shortcuts")
}
