//! The columns of the file list: which there are, in what order, how wide,
//! what each cell says, and the header that lets a person change them.
//!
//! The module has three parts.
//!
//! - The model: [`Column`], [`ColumnLayout`] (order, widths and visibility,
//!   remembered between runs) and [`plan`], which turns a layout into the
//!   widths to draw for a given room. All of it is pure.
//! - The text: [`cell_text`] says what a column shows for an entry.
//! - The view: [`ColumnHeaders`] and the box maths that rows share with it.
//!
//! # Mounting the header
//!
//! A header and every row are one flex row with the same horizontal padding
//! ([`row_padding`]) and the same children, so the cells line up exactly:
//!
//! ```text
//! | gutter(icon_px) | Name (flex, min plan.name_min) | plan.columns[0] | plan.columns[1] | ...
//! ```
//!
//! - The gutter is the icon slot. A row draws its icon in a box
//!   `gutter(icon_px)` wide. [`ColumnHeaders`] leaves the same space empty.
//! - The Name cell is [`name_cell`]. It flexes and never goes below
//!   `plan.name_min`.
//! - Each other cell is [`cell`], `width` taken from [`row_cell_widths`].
//!
//! There is no gap between cells. Each cell pads its own content by
//! [`CELL_PADDING`], so the text of a header and of its column start at the
//! same x. [`plan_in`] subtracts the gutter for a measured row width.
//!
//! The columns do not know the entries: the owner of the list supplies a
//! `fit` function that measures the widest cell of a column.

// Until `file_list.rs` uses the module.
#![allow(dead_code)]

use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bingux_files::display::{self, Civil};
use bingux_files::{Entry, Kind as EntryKind, Sort, SortKey};
use freya::prelude::*;
use serde::{Deserialize, Serialize};

use super::view_state;
use crate::component::ComponentName;
use crate::controls::focus_stop_press;
use crate::controls::press::point_of;
use crate::icons::{self, Icon};
use crate::shell::{MenuItem, menu_item, menu_separator};
use crate::style::*;
use crate::surface::use_tokens;
use crate::typography::{CAPTION, Ink};

/// The key the layout is remembered under in `view_state`.
const STATE_KEY: &str = "columns";

/// The widest a column may be, in logical pixels.
const MAX_WIDTH: f32 = 800.0;
/// The width of Name when the room is tight, in logical pixels.
const NAME_MIN_WIDTH: f32 = 160.0;
/// How far Ctrl+Left and Ctrl+Right resize a column, in logical pixels.
const KEY_RESIZE_STEP: f32 = 8.0;
/// How far the pointer travels before a press on a header becomes a drag.
const DRAG_SLOP: f32 = 6.0;
/// The width of the resize handle, centred on a column's left edge.
const HANDLE_PX: f32 = 6.0;
/// How far above its cell the resize handle sits, in layers. The heading's
/// content is nested three deep, so anything lower loses the press to it.
const HANDLE_LAYER: i16 = 8;
/// Two presses on one handle this close together fit the column.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// The horizontal space between the edge of the list and a row's content: the
/// list's edge and the inset inside the row's fill ([`FILE_ROW_INSET`], which
/// does not grow on a phone). Header and rows must use the same, or their cells
/// drift apart; and it is the same at every width, so an icon's left edge is.
pub fn row_padding() -> f32 {
    LIST_EDGE.px() + FILE_ROW_INSET.px()
}
/// The space a cell keeps at each side of its content.
pub const CELL_PADDING: Space = S2;
/// The space between the icon slot and the Name cell, before Name's own
/// padding.
const GUTTER_GAP: Space = S1;

/// A column of the file list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    Name,
    Size,
    Kind,
    Modified,
    Created,
    Accessed,
    Permissions,
    Owner,
    /// The folder an entry is in. Only Recent and search results have it.
    Location,
    /// The folder an item of the Trash was in before it was deleted. Only the
    /// Trash has it.
    OriginalLocation,
    /// When an item of the Trash was deleted. Only the Trash has it.
    Deleted,
}

/// Which columns a list may show: an ordinary folder's, Recent's and search
/// results' (which add where each item is), or the Trash's (which show where
/// each item was and when it was deleted, in place of its type and date).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnSet {
    Folder,
    Results,
    Trash,
}

impl From<bool> for ColumnSet {
    /// `true` for the views that list items from several folders.
    fn from(with_location: bool) -> ColumnSet {
        if with_location {
            ColumnSet::Results
        } else {
            ColumnSet::Folder
        }
    }
}

impl Column {
    /// Whether `set` may show this column at all.
    pub fn in_set(self, set: ColumnSet) -> bool {
        let about_the_trash = matches!(self, Column::OriginalLocation | Column::Deleted);
        match set {
            ColumnSet::Folder => !about_the_trash && self != Column::Location,
            ColumnSet::Results => !about_the_trash,
            ColumnSet::Trash => matches!(self, Column::Size) || about_the_trash,
        }
    }

    /// Whether `set` shows this column whatever the layout's tick says.
    fn always_in(self, set: ColumnSet) -> bool {
        set == ColumnSet::Trash && matches!(self, Column::OriginalLocation | Column::Deleted)
    }
}

impl Column {
    /// Every column, in the order a fresh layout starts with.
    pub const ALL: [Column; 11] = [
        Column::Name,
        Column::Size,
        Column::Kind,
        Column::Modified,
        Column::Created,
        Column::Accessed,
        Column::Permissions,
        Column::Owner,
        Column::Location,
        Column::OriginalLocation,
        Column::Deleted,
    ];

    /// The heading and the name in menus.
    pub fn title(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::Size => "Size",
            Column::Kind => "Type",
            Column::Modified => "Modified",
            Column::Created => "Created",
            Column::Accessed => "Accessed",
            Column::Permissions => "Permissions",
            Column::Owner => "Owner",
            Column::Location => "Location",
            Column::OriginalLocation => "Original Location",
            Column::Deleted => "Deleted",
        }
    }

    /// The order a click on the heading sorts by, if the model can sort by
    /// this column.
    pub fn sort_key(self) -> Option<SortKey> {
        match self {
            Column::Name => Some(SortKey::Name),
            Column::Size => Some(SortKey::Size),
            Column::Kind => Some(SortKey::Kind),
            Column::Modified => Some(SortKey::Modified),
            Column::Created
            | Column::Accessed
            | Column::Permissions
            | Column::Owner
            | Column::Location
            | Column::OriginalLocation => None,
            Column::Deleted => Some(SortKey::Deleted),
        }
    }

    /// The width a new layout gives the column, in logical pixels.
    /// Name flexes, so its number is only the room it asks for.
    pub fn default_width(self) -> f32 {
        match self {
            Column::Name => NAME_MIN_WIDTH,
            Column::Size => 88.0,
            Column::Kind => 140.0,
            Column::Modified => 120.0,
            Column::Created => 120.0,
            Column::Accessed => 120.0,
            Column::Permissions => 104.0,
            Column::Owner => 96.0,
            Column::Location => 220.0,
            Column::OriginalLocation => 220.0,
            Column::Deleted => 130.0,
        }
    }

    /// The narrowest the column may be made, in logical pixels.
    pub fn min_width(self) -> f32 {
        match self {
            Column::Name => NAME_MIN_WIDTH,
            Column::Size => 72.0,
            Column::Kind => 72.0,
            Column::Modified => 80.0,
            Column::Created => 80.0,
            Column::Accessed => 80.0,
            Column::Permissions => 88.0,
            Column::Owner => 56.0,
            Column::Location => 96.0,
            Column::OriginalLocation => 96.0,
            Column::Deleted => 96.0,
        }
    }

    /// Numbers read best against the right edge.
    pub fn right_aligned(self) -> bool {
        matches!(self, Column::Size)
    }

    fn visible_by_default(self) -> bool {
        matches!(
            self,
            Column::Name | Column::Size | Column::Modified | Column::Location
        )
    }

    /// The name the column is saved under.
    fn key(self) -> &'static str {
        match self {
            Column::Name => "name",
            Column::Size => "size",
            Column::Kind => "kind",
            Column::Modified => "modified",
            Column::Created => "created",
            Column::Accessed => "accessed",
            Column::Permissions => "permissions",
            Column::Owner => "owner",
            Column::Location => "location",
            Column::OriginalLocation => "original_location",
            Column::Deleted => "deleted",
        }
    }

    fn from_key(key: &str) -> Option<Column> {
        Column::ALL.into_iter().find(|column| column.key() == key)
    }

    /// `width` made to fit the column's limits. A number that is not a
    /// number becomes the default.
    fn sane_width(self, width: f32) -> f32 {
        if width.is_finite() {
            width.clamp(self.min_width(), MAX_WIDTH.max(self.min_width()))
        } else {
            self.default_width()
        }
    }
}

/// One optional column in a layout.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct Slot {
    column: Column,
    /// Logical pixels.
    width: f32,
    visible: bool,
}

impl Slot {
    fn fresh(column: Column) -> Slot {
        Slot {
            column,
            width: column.default_width(),
            visible: column.visible_by_default(),
        }
    }
}

/// A saved slot as the file holds it. The column is a plain string, so a
/// column this build does not know can be skipped instead of spoiling the
/// whole layout.
#[derive(Deserialize)]
struct RawSlot {
    column: String,
    #[serde(default)]
    width: f32,
    #[serde(default = "yes")]
    visible: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct RawLayout {
    #[serde(default)]
    columns: Vec<RawSlot>,
}

/// Which columns the list shows, in what order and how wide.
///
/// Name is always first and always shown, and it takes the room the others
/// leave. It is not part of the layout's slots: it cannot be moved, hidden
/// or resized. Every other column can be.
///
/// A layout is remembered between runs: [`load`](Self::load) reads it and
/// [`save`](Self::save) writes it. A file from another version is repaired:
/// columns it does not know are dropped, columns it lacks are added in their
/// default place, and widths are put back in range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawLayout")]
pub struct ColumnLayout {
    columns: Vec<Slot>,
}

impl From<RawLayout> for ColumnLayout {
    fn from(raw: RawLayout) -> Self {
        let mut columns: Vec<Slot> = Vec::with_capacity(Column::ALL.len());
        for slot in raw.columns {
            let Some(column) = Column::from_key(&slot.column) else {
                continue;
            };
            if column == Column::Name || columns.iter().any(|known| known.column == column) {
                continue;
            }
            columns.push(Slot {
                column,
                width: column.sane_width(slot.width),
                visible: slot.visible,
            });
        }
        for column in Column::ALL {
            if column != Column::Name && !columns.iter().any(|known| known.column == column) {
                columns.push(Slot::fresh(column));
            }
        }
        ColumnLayout { columns }
    }
}

impl Default for ColumnLayout {
    fn default() -> Self {
        ColumnLayout {
            columns: Column::ALL
                .into_iter()
                .filter(|column| *column != Column::Name)
                .map(Slot::fresh)
                .collect(),
        }
    }
}

impl ColumnLayout {
    /// The remembered layout, or the default.
    pub fn load() -> Self {
        view_state::load::<ColumnLayout>(STATE_KEY).unwrap_or_default()
    }

    /// Remember the layout. The write is delayed, so this is cheap enough to
    /// call when a change ends.
    pub fn save(&self) {
        view_state::save(STATE_KEY, self);
    }

    /// Every column in order, Name first, shown or not.
    pub fn order(&self) -> Vec<Column> {
        std::iter::once(Column::Name)
            .chain(self.columns.iter().map(|slot| slot.column))
            .collect()
    }

    /// The shown columns after Name, in order. Location is left out unless
    /// `with_location`.
    pub fn visible(&self, set: impl Into<ColumnSet>) -> Vec<Column> {
        let set = set.into();
        self.columns
            .iter()
            .filter(|slot| slot.column.in_set(set) && (slot.visible || slot.column.always_in(set)))
            .map(|slot| slot.column)
            .collect()
    }

    /// Whether `column` is shown. Name always is.
    pub fn is_visible(&self, column: Column) -> bool {
        column == Column::Name || self.slot(column).is_some_and(|slot| slot.visible)
    }

    /// The width of `column` in logical pixels. Name's is the
    /// room it asks for.
    pub fn width(&self, column: Column) -> f32 {
        self.slot(column)
            .map_or_else(|| column.default_width(), |slot| slot.width)
    }

    /// Set the width, kept within the column's limits. Name cannot be
    /// resized.
    pub fn resize(&mut self, column: Column, width: f32) {
        if let Some(slot) = self.slot_mut(column) {
            slot.width = column.sane_width(width);
        }
    }

    /// Make the column as wide as its widest cell, `content_width` logical
    /// pixels including padding, kept within the column's limits.
    pub fn fit(&mut self, column: Column, content_width: f32) {
        self.resize(column, content_width);
    }

    /// Put `column` at `to_index` in [`order`](Self::order), where Name is
    /// index 0. Nothing goes before Name, and Name does not move.
    pub fn move_column(&mut self, column: Column, to_index: usize) {
        let Some(from) = self.columns.iter().position(|slot| slot.column == column) else {
            return;
        };
        let slot = self.columns.remove(from);
        let at = to_index.saturating_sub(1).min(self.columns.len());
        self.columns.insert(at, slot);
    }

    /// Put `column` so that it is the `slot`th of the columns `among` (the
    /// shown columns after Name, in order), counting the others only. A slot
    /// past the end puts it after the last. Columns not in `among` (hidden
    /// ones) keep their places.
    pub fn move_to_slot(&mut self, column: Column, slot: usize, among: &[Column]) {
        let Some(from) = self.columns.iter().position(|known| known.column == column) else {
            return;
        };
        let others: Vec<Column> = among.iter().copied().filter(|c| *c != column).collect();
        let moving = self.columns.remove(from);
        let at = match others.get(slot) {
            Some(before) => self.columns.iter().position(|known| known.column == *before),
            None => others
                .last()
                .and_then(|last| self.columns.iter().position(|known| known.column == *last))
                .map(|index| index + 1),
        };
        match at {
            Some(at) => self.columns.insert(at, moving),
            None => self.columns.insert(from.min(self.columns.len()), moving),
        }
    }

    /// Show or hide a column. Name cannot be hidden.
    pub fn set_visible(&mut self, column: Column, visible: bool) {
        if let Some(slot) = self.slot_mut(column) {
            slot.visible = visible;
        }
    }

    /// The default order, widths and visibility.
    pub fn reset(&mut self) {
        *self = ColumnLayout::default();
    }

    fn slot(&self, column: Column) -> Option<&Slot> {
        self.columns.iter().find(|slot| slot.column == column)
    }

    fn slot_mut(&mut self, column: Column) -> Option<&mut Slot> {
        self.columns.iter_mut().find(|slot| slot.column == column)
    }
}

/// The widths to draw, from [`plan`].
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// The least width of the Name cell.
    pub name_min: f32,
    /// The columns after Name that fit, in order, with the width to draw
    /// each at.
    pub columns: Vec<(Column, f32)>,
    /// How many shown columns did not fit and were left out, from the right.
    pub hidden: usize,
    /// Whether the Location column was a candidate.
    pub with_location: bool,
    /// The columns the list is drawn with.
    pub set: ColumnSet,
}

/// The widths to draw in `available` pixels.
///
/// `available` is what Name and the other cells share: the row's content
/// width less the icon [`gutter`]. A width of zero or less means the room is
/// not measured yet, and every shown column is kept.
///
/// Name takes what the others leave, and not less than 160 pixels. When the
/// shown columns do not fit with that, the rightmost optional columns are
/// left out until they do. Name is never left out. The widths do not depend
/// on the zoom: the text does not change size, only the icon does.
pub fn plan(layout: &ColumnLayout, available: f32, set: impl Into<ColumnSet>) -> Plan {
    let set = set.into();
    let name_min = NAME_MIN_WIDTH;
    let mut columns: Vec<(Column, f32)> = layout
        .visible(set)
        .into_iter()
        .map(|column| (column, layout.width(column).max(column.min_width())))
        .collect();
    let mut hidden = 0;
    if available > 0.0 {
        let room = available - name_min;
        while columns.iter().map(|(_, width)| width).sum::<f32>() > room {
            if columns.pop().is_none() {
                break;
            }
            hidden += 1;
        }
    }
    Plan {
        name_min,
        columns,
        hidden,
        with_location: set == ColumnSet::Results,
        set,
    }
}

/// [`plan`] for a measured row of `row_width` pixels whose icon is `icon_px`
/// wide: the icon [`gutter`] is taken off first. Call it while rendering, as
/// the gutter gap reads the size class.
pub fn plan_in(layout: &ColumnLayout, icon_px: f32, row_width: f32, set: impl Into<ColumnSet>) -> Plan {
    let available = if row_width > 0.0 {
        (row_width - gutter(icon_px)).max(1.0)
    } else {
        0.0
    };
    plan(layout, available, set)
}

/// The width of the icon slot that opens a row: the icon, `icon_px` wide,
/// plus a small gap. A header and its rows both start with a box this wide.
/// Read the size class: call it while rendering.
pub fn gutter(icon_px: f32) -> f32 {
    icon_px + GUTTER_GAP.px()
}

/// The widths of the cells after Name, left to right: exactly what a row
/// draws, so its cells match the header's.
pub fn row_cell_widths(plan: &Plan) -> &[(Column, f32)] {
    &plan.columns
}

/// A fixed-width cell of a row, with the content padded by [`CELL_PADDING`]
/// and centred up and down. `right` aligns the content to the right edge, as
/// numbers want. Give it a label with `max_lines(1)` and an ellipsis.
pub fn cell(width: f32, right: bool, child: impl IntoElement) -> Rect {
    rect()
        .width(Size::px(width))
        .tw_layout(&[
            flex_col(),
            justify(Align::Center),
            items(if right { Align::End } else { Align::Start }),
            px(CELL_PADDING),
            h_full(),
        ])
        .child(child)
}

/// The Name cell of a row: it flexes into the room the other cells leave and
/// keeps at least `plan.name_min`.
pub fn name_cell(plan: &Plan, child: impl IntoElement) -> Rect {
    rect()
        .min_width(Size::px(plan.name_min))
        .tw_layout(&[
            flex_col(),
            justify(Align::Center),
            items(Align::Start),
            w_flex(),
            px(CELL_PADDING),
            h_full(),
        ])
        .child(child)
}

// ----------------------------------------------------------------------------
// Cell text
// ----------------------------------------------------------------------------

/// How a time column writes a date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateStyle {
    /// `12 Mar` this year, `12 Mar 2025` before.
    Absolute,
    /// `Just now`, `12 minutes ago`, `3 hours ago`, `Yesterday 14:03`, `Mon
    /// 14:03` within a week, then absolute. For Recent, where the question
    /// is how long ago.
    Relative,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// FilesModel is adding `sized`, `created`, `accessed`, `mode` and `owner` to
// `Entry` (see /var/tmp/FilesModel/API.md). Until they land these shims stand
// in for them. Switch each body to the field and delete this note.
fn sized(_entry: &Entry) -> bool {
    true
}
fn created(_entry: &Entry) -> Option<SystemTime> {
    None
}
fn accessed(_entry: &Entry) -> Option<SystemTime> {
    None
}
fn mode(_entry: &Entry) -> Option<u32> {
    None
}
fn owner(_entry: &Entry) -> String {
    String::new()
}

/// What `column` shows for `entry`, with dates written absolutely. `now` is
/// the time to measure relative dates from; `home` shortens a location to
/// `~/...`. Nothing here reads the disk or the clock.
pub fn cell_text(column: Column, entry: &Entry, now: SystemTime, home: &Path) -> String {
    cell_text_styled(column, entry, now, home, DateStyle::Absolute)
}

/// [`cell_text`] with the date style chosen: Recent wants
/// [`DateStyle::Relative`].
pub fn cell_text_styled(
    column: Column,
    entry: &Entry,
    now: SystemTime,
    home: &Path,
    dates: DateStyle,
) -> String {
    text_with(column, entry, now, home, dates, &display::local_offset)
}

/// The cell text, with the zone offset as a parameter so tests need not
/// depend on the machine's zone.
fn text_with(
    column: Column,
    entry: &Entry,
    now: SystemTime,
    home: &Path,
    dates: DateStyle,
    offset: &dyn Fn(i64) -> i64,
) -> String {
    let date = |time: Option<SystemTime>| {
        let Some(time) = time else {
            return String::new();
        };
        let (then, now) = (unix_seconds(time), unix_seconds(now));
        let offset = offset(then);
        match dates {
            DateStyle::Absolute => absolute_date(then, now, offset),
            DateStyle::Relative => relative_date(then, now, offset),
        }
    };
    match column {
        Column::Name => entry.name.clone(),
        Column::Size => {
            if entry.is_dir() {
                String::new()
            } else if !sized(entry) {
                "\u{2013}".to_string()
            } else {
                display::size(entry.size)
            }
        }
        Column::Kind => entry.mime.description.to_string(),
        Column::Modified => date(entry.modified),
        Column::Created => date(created(entry)),
        Column::Accessed => date(accessed(entry)),
        Column::Permissions => mode(entry)
            .map(|mode| permissions_text(mode, entry.kind, entry.symlink))
            .unwrap_or_default(),
        Column::Owner => owner(entry),
        Column::Location => location_text(&entry.path, home),
        // Where it was, as the folder it was in; nothing is known when the
        // info file is missing. The Trash says how long ago, as Recent does.
        Column::OriginalLocation => match entry.trash.as_ref() {
            Some(trash) => trash
                .original
                .as_deref()
                .map_or_else(|| "Unknown".to_owned(), |original| location_text(original, home)),
            None => String::new(),
        },
        Column::Deleted => {
            let Some(time) = entry.trash.as_ref().and_then(|trash| trash.deleted) else {
                return String::new();
            };
            let (then, now) = (unix_seconds(time), unix_seconds(now));
            relative_date(then, now, offset(then))
        }
    }
}

fn unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_secs() as i64,
        Err(before) => -(before.duration().as_secs() as i64),
    }
}

/// `12 Mar` in the year of `now`, `12 Mar 2025` in another.
fn absolute_date(seconds: i64, now: i64, offset: i64) -> String {
    let then = Civil::from_unix(seconds, offset);
    let today = Civil::from_unix(now, offset);
    let month = MONTHS[then.month as usize - 1];
    if then.year == today.year {
        format!("{} {month}", then.day)
    } else {
        format!("{} {month} {}", then.day, then.year)
    }
}

fn relative_date(seconds: i64, now: i64, offset: i64) -> String {
    let elapsed = now - seconds;
    if elapsed < 0 {
        // A clock a little behind the file's is not worth a date.
        return if elapsed > -60 {
            "Just now".into()
        } else {
            absolute_date(seconds, now, offset)
        };
    }
    let plural = |count: i64, unit: &str| {
        if count == 1 {
            format!("1 {unit} ago")
        } else {
            format!("{count} {unit}s ago")
        }
    };
    if elapsed < 60 {
        return "Just now".into();
    }
    if elapsed < 3600 {
        return plural(elapsed / 60, "minute");
    }
    if elapsed < 6 * 3600 {
        return plural(elapsed / 3600, "hour");
    }
    let then = Civil::from_unix(seconds, offset);
    let today = Civil::from_unix(now, offset);
    let clock = format!("{:02}:{:02}", then.hour, then.minute);
    match today.day_number - then.day_number {
        0 => format!("Today {clock}"),
        1 => format!("Yesterday {clock}"),
        2..=6 => {
            let weekday = (then.day_number + 3).rem_euclid(7) as usize;
            format!("{} {clock}", WEEKDAYS[weekday])
        }
        _ => absolute_date(seconds, now, offset),
    }
}

/// `drwxr-xr-x`: the type letter, then read, write and execute for the
/// owner, the group and everyone else. Set-id and sticky bits show as `s`
/// and `t` (a capital when execute is off).
fn permissions_text(mode: u32, kind: EntryKind, symlink: bool) -> String {
    let mut text = String::with_capacity(10);
    text.push(if symlink {
        'l'
    } else {
        match kind {
            EntryKind::Directory => 'd',
            EntryKind::File => '-',
            EntryKind::Other => '?',
        }
    });
    // (read, write, execute, special bit, letter for the special bit)
    let triples = [
        (0o400, 0o200, 0o100, 0o4000, 's'),
        (0o040, 0o020, 0o010, 0o2000, 's'),
        (0o004, 0o002, 0o001, 0o1000, 't'),
    ];
    for (read, write, execute, special, letter) in triples {
        text.push(if mode & read != 0 { 'r' } else { '-' });
        text.push(if mode & write != 0 { 'w' } else { '-' });
        text.push(match (mode & execute != 0, mode & special != 0) {
            (true, true) => letter,
            (false, true) => letter.to_ascii_uppercase(),
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    text
}

/// The folder holding `path`, with `home` written `~`.
fn location_text(path: &Path, home: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    match parent.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => parent.display().to_string(),
    }
}

// ----------------------------------------------------------------------------
// The column menu
// ----------------------------------------------------------------------------

/// The items of the columns menu: a tick for each column that is shown, Name
/// greyed out because it always is, and `Reset columns`. `with_location`
/// lists Location too, for the views that have it. Every change is saved.
pub fn columns_menu_items(layout: State<ColumnLayout>, set: impl Into<ColumnSet>) -> Vec<MenuItem> {
    let set = set.into();
    // The Trash shows the columns that are about the Trash and nothing to
    // choose from.
    if set == ColumnSet::Trash {
        return Vec::new();
    }
    let mut items = Vec::new();
    for column in Column::ALL {
        if !column.in_set(set) {
            continue;
        }
        let shown = layout.peek().is_visible(column);
        let item = menu_item(column.title(), move |_| {
            let mut layout = layout;
            let mut next = layout.peek().clone();
            let now = next.is_visible(column);
            next.set_visible(column, !now);
            next.save();
            layout.set(next);
        })
        .checked(shown);
        items.push(if column == Column::Name {
            item.disabled()
        } else {
            item
        });
    }
    items.push(menu_separator());
    items.push(menu_item("Reset columns", move |_| {
        let mut layout = layout;
        let mut next = layout.peek().clone();
        next.reset();
        next.save();
        layout.set(next);
    }));
    items
}

// ----------------------------------------------------------------------------
// Drag maths
// ----------------------------------------------------------------------------

/// Where a column dragged `dx` pixels lands among its neighbours: its index
/// in the order with the dragged column taken out. `widths` are the drawn
/// widths of all the columns after Name, `dragged` the index of the one
/// moving. The column lands after every other column whose centre it has
/// passed.
fn drop_slot(widths: &[f32], dragged: usize, dx: f32) -> usize {
    let centres = centres(widths);
    let Some(own) = centres.get(dragged) else {
        return 0;
    };
    let centre = own + dx;
    centres
        .iter()
        .enumerate()
        .filter(|(index, other)| *index != dragged && **other < centre)
        .count()
}

fn centres(widths: &[f32]) -> Vec<f32> {
    let mut left = 0.0;
    widths
        .iter()
        .map(|width| {
            let centre = left + width / 2.0;
            left += width;
            centre
        })
        .collect()
}

/// Where the drop line goes for `slot`, from the left edge of the first
/// column after Name: before the `slot`th of the other columns, which have
/// not moved, or after the last.
fn line_position(widths: &[f32], dragged: usize, slot: usize) -> f32 {
    let mut left = 0.0;
    let mut edges = Vec::with_capacity(widths.len());
    for (index, width) in widths.iter().enumerate() {
        edges.push((index, left, left + width));
        left += width;
    }
    let others: Vec<_> = edges.iter().filter(|(index, ..)| *index != dragged).collect();
    match others.get(slot) {
        Some((_, start, _)) => *start,
        None => others.last().map_or(0.0, |(_, _, end)| *end),
    }
}

// ----------------------------------------------------------------------------
// The header
// ----------------------------------------------------------------------------

/// What the pointer is doing to the header.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Pressed on a heading; a move past the slop turns it into a `Move`.
    Pending { column: Column, start_x: f32 },
    /// Dragging a heading to another place.
    Move {
        column: Column,
        start_x: f32,
        dx: f32,
    },
    /// Dragging a column's left edge. `start_width` is logical pixels.
    Resize {
        column: Column,
        start_x: f32,
        start_width: f32,
    },
}

/// The function that measures a column: the widest text its cells show, in
/// drawn pixels, padding not included. The owner of the list supplies it
/// from the entries; the header calls it on a double click on a column's
/// edge.
#[derive(Clone)]
pub struct Fit(pub Rc<dyn Fn(Column) -> f32>);

impl PartialEq for Fit {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Rc::as_ptr(&self.0), Rc::as_ptr(&other.0))
    }
}

/// What every cell of the header shares.
#[derive(Clone, PartialEq)]
struct Shared {
    layout: State<ColumnLayout>,
    drag: State<Option<Drag>>,
    /// Set when the press that follows a gesture must not sort: a right
    /// click that opened the menu, or a heading that was dragged.
    ignore_press: State<bool>,
    last_edge_press: State<Option<(Instant, Column)>>,
    sort: Sort,
    on_sort: EventHandler<SortKey>,
    on_menu: EventHandler<(f32, f32)>,
    fit: Fit,
    /// The columns drawn after Name, in order.
    drawn: Vec<Column>,
}

impl Shared {
    /// Change the layout. Returns whether it differed.
    fn edit(&self, change: impl FnOnce(&mut ColumnLayout)) -> bool {
        let mut layout = self.layout;
        let mut next = layout.peek().clone();
        change(&mut next);
        let changed = next != *layout.peek();
        layout.set_if_modified(next);
        changed
    }

    fn save(&self) {
        self.layout.peek().save();
    }
}

/// The header row of the file list: one heading for each column, over the
/// cells the rows draw.
///
/// A click sorts. A column's left edge drags to resize it (left widens, as
/// Name flexes and the columns hang from the right), and a double click
/// there fits the column to its widest cell. Dragging a heading moves the
/// column. A right click asks for the columns menu. From the keyboard, a
/// focused heading moves with Shift+Left and Shift+Right and resizes with
/// Ctrl+Left and Ctrl+Right. Every change is saved when the gesture ends.
///
/// See the module documentation for how it lines up with the rows.
#[derive(PartialEq)]
pub struct ColumnHeaders {
    layout: State<ColumnLayout>,
    plan: Plan,
    sort: Sort,
    on_sort: EventHandler<SortKey>,
    on_menu: EventHandler<(f32, f32)>,
    fit: Fit,
    icon_px: f32,
}

impl ColumnHeaders {
    /// A header for `plan`, which came from `layout` through [`plan`].
    pub fn new(layout: State<ColumnLayout>, plan: Plan, sort: Sort) -> Self {
        ColumnHeaders {
            layout,
            plan,
            sort,
            on_sort: EventHandler::new(|_: SortKey| {}),
            on_menu: EventHandler::new(|_: (f32, f32)| {}),
            fit: Fit(Rc::new(|_| 0.0)),
            icon_px: super::zoom::DEFAULT_PX as f32,
        }
    }

    /// The width of the icons of the rows, for the space the header leaves
    /// for them.
    pub fn icon_px(mut self, icon_px: f32) -> Self {
        self.icon_px = icon_px;
        self
    }

    /// A heading was pressed: sort by this key.
    pub fn on_sort(mut self, handler: impl Into<EventHandler<SortKey>>) -> Self {
        self.on_sort = handler.into();
        self
    }

    /// A right click, or the Menu key, on the header: open the columns menu
    /// (see [`columns_menu_items`]) at this window point.
    pub fn on_menu(mut self, handler: impl Into<EventHandler<(f32, f32)>>) -> Self {
        self.on_menu = handler.into();
        self
    }

    /// How to measure a column for a double click on its edge. Without it
    /// the double click shrinks the column to its least width.
    pub fn fit(mut self, fit: Rc<dyn Fn(Column) -> f32>) -> Self {
        self.fit = Fit(fit);
        self
    }
}

impl Component for ColumnHeaders {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let drag = use_state(|| None::<Drag>);
        let ignore_press = use_state(|| false);
        let last_edge_press = use_state(|| None::<(Instant, Column)>);
        let widths: Vec<f32> = self.plan.columns.iter().map(|(_, width)| *width).collect();
        let drawn: Vec<Column> = self.plan.columns.iter().map(|(column, _)| *column).collect();
        let shared = Shared {
            layout: self.layout,
            drag,
            ignore_press,
            last_edge_press,
            sort: self.sort,
            on_sort: self.on_sort.clone(),
            on_menu: self.on_menu.clone(),
            fit: self.fit.clone(),
            drawn: drawn.clone(),
        };

        let on_move = {
            let shared = shared.clone();
            move |event: Event<PointerEventData>| {
                let Some(current) = *shared.drag.peek() else {
                    return;
                };
                let x = event.global_location().x as f32;
                let mut drag = shared.drag;
                match current {
                    Drag::Pending { column, start_x } => {
                        if (x - start_x).abs() > DRAG_SLOP {
                            let mut ignore = shared.ignore_press;
                            ignore.set(true);
                            drag.set(Some(Drag::Move {
                                column,
                                start_x,
                                dx: x - start_x,
                            }));
                            event.prevent_default();
                        }
                    }
                    Drag::Move { column, start_x, .. } => {
                        drag.set_if_modified(Some(Drag::Move {
                            column,
                            start_x,
                            dx: x - start_x,
                        }));
                        event.prevent_default();
                    }
                    Drag::Resize {
                        column,
                        start_x,
                        start_width,
                    } => {
                        // Name flexes, so the other columns hang from the right edge
                        // and a column's left edge is the one that follows the pointer:
                        // dragging left widens.
                        let width = start_width - (x - start_x);
                        shared.edit(|layout| layout.resize(column, width));
                        event.prevent_default();
                    }
                }
            }
        };
        let on_up = {
            let shared = shared.clone();
            let widths = widths.clone();
            let drawn = drawn.clone();
            move |_: Event<PointerEventData>| {
                let Some(current) = *shared.drag.peek() else {
                    return;
                };
                let mut drag = shared.drag;
                drag.set(None);
                match current {
                    Drag::Pending { .. } => {}
                    Drag::Move { column, dx, .. } => {
                        if let Some(index) = drawn.iter().position(|known| *known == column) {
                            let slot = drop_slot(&widths, index, dx);
                            shared.edit(|layout| layout.move_to_slot(column, slot, &drawn));
                        }
                        shared.save();
                    }
                    Drag::Resize { .. } => shared.save(),
                }
            }
        };

        let current = *drag.read();
        let moving = match current {
            Some(Drag::Move { column, dx, .. }) => Some((column, dx)),
            _ => None,
        };
        let cell_of = |column: Column, width: Option<f32>| {
            let offset = match moving {
                Some((moved, dx)) if moved == column => {
                    let Some(index) = drawn.iter().position(|known| *known == column) else {
                        return Element::from(HeaderCell {
                            column,
                            width,
                            name_min: self.plan.name_min,
                            offset: 0.0,
                            moving: false,
                            shared: shared.clone(),
                        });
                    };
                    // The heading stays inside the columns.
                    let before: f32 = widths[..index].iter().sum();
                    let total: f32 = widths.iter().sum();
                    dx.clamp(-before, total - before - widths[index])
                }
                _ => 0.0,
            };
            Element::from(HeaderCell {
                column,
                width,
                name_min: self.plan.name_min,
                offset,
                moving: moving.is_some_and(|(moved, _)| moved == column),
                shared: shared.clone(),
            })
        };

        let line = moving.and_then(|(column, dx)| {
            let index = drawn.iter().position(|known| *known == column)?;
            let slot = drop_slot(&widths, index, dx);
            let at = line_position(&widths, index, slot);
            Some(
                rect()
                    .position(Position::new_absolute().top(0.0).left(at - 1.0))
                    .layer(Layer::Relative(2))
                    .tw(&tokens, &[w(Dim::Indicator), bg(Paint::Accent)])
                    .height(Size::percent(100.0)),
            )
        });

        let total: f32 = widths.iter().sum();
        let strip = rect()
            .width(Size::px(total))
            .tw(&tokens, &[flex_row(), h_full()])
            .children(
                self.plan
                    .columns
                    .iter()
                    .map(|(column, width)| cell_of(*column, Some(*width))),
            )
            .maybe_child(line);

        // The row's own two paddings: the list's edge, then the inset inside
        // a row's fill (see `file_list.rs`), so every heading sits over its
        // column.
        rect()
            .component("ColumnHeaders")
            .tw(&tokens, &[flex_row(), w_full(), h(Dim::Control), px(LIST_EDGE)])
            .on_capture_global_pointer_move(on_move)
            .on_global_pointer_up(on_up)
            .child(
                rect()
                    .tw(
                        &tokens,
                        &[
                            flex_row(),
                            grow_children(),
                            items(Align::Center),
                            w_full(),
                            h_full(),
                            px(FILE_ROW_INSET),
                        ],
                    )
                    .child(rect().width(Size::px(gutter(self.icon_px))))
                    .child(cell_of(Column::Name, None))
                    .child(strip),
            )
    }
}

/// One heading with its resize handle.
#[derive(PartialEq)]
struct HeaderCell {
    column: Column,
    /// `None` for Name, which flexes.
    width: Option<f32>,
    name_min: f32,
    /// How far a dragged heading is shifted from its place.
    offset: f32,
    moving: bool,
    shared: Shared,
}

impl Component for HeaderCell {
    fn render_key(&self) -> DiffKey {
        DiffKey::U64(self.column as u8 as u64)
    }

    fn render(&self) -> impl IntoElement {
        let direction = crate::controls::direction::use_direction();
        let tokens = use_tokens();
        let mut hovering = use_state(|| false);
        let mut pressing = use_state(|| false);
        let mut edge_hot = use_state(|| false);
        let mut area = use_state(Area::default);
        let column = self.column;
        let shared = self.shared.clone();
        let sorted = column
            .sort_key()
            .and_then(|key| (shared.sort.key == key).then_some(shared.sort.descending));
        let resizing = matches!(
            *shared.drag.read(),
            Some(Drag::Resize { column: sized, .. }) if sized == column
        );

        let arrow = sorted.map(|descending| {
            icons::icon(
                if descending {
                    Icon::PanDown
                } else {
                    Icon::PanUp
                },
                IconSize::Mark,
                Paint::Muted.color(&tokens),
            )
        });
        let label = CAPTION
            .in_ink(if sorted.is_some() { Ink::Text } else { Ink::Muted })
            .label(&tokens, column.title())
            .max_lines(1)
            .text_overflow(TextOverflow::Ellipsis);

        // The pointer-down handler lives on this child, not on the focus
        // stop: the stop keeps its own and swallows the event after this
        // child has seen it.
        let body = {
            let shared = shared.clone();
            rect()
                .tw(
                    &tokens,
                    &[
                        flex_row(),
                        items(Align::Center),
                        justify(if column.right_aligned() {
                            Align::End
                        } else {
                            Align::Start
                        }),
                        w_full(),
                        h_full(),
                        px(CELL_PADDING),
                        gap(S1),
                    ],
                )
                .on_pointer_down(move |event: Event<PointerEventData>| {
                    let PointerEventData::Mouse(mouse) = event.data() else {
                        return;
                    };
                    let mut ignore = shared.ignore_press;
                    let mut drag = shared.drag;
                    match mouse.button {
                        Some(MouseButton::Right) => {
                            ignore.set(true);
                            shared.on_menu.call(point_of(&event));
                        }
                        Some(MouseButton::Left) => {
                            ignore.set(false);
                            if column != Column::Name {
                                drag.set(Some(Drag::Pending {
                                    column,
                                    start_x: event.global_location().x as f32,
                                }));
                            }
                        }
                        _ => {}
                    }
                })
                .child(label)
                .maybe_child(arrow)
        };

        let content = rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    rounded(Radius::Mark),
                    surface(Paint::Transparent, hovering(), self.moving || pressing(), false),
                ],
            )
            .on_pointer_enter(move |_| hovering.set(true))
            .on_pointer_leave(move |_| {
                hovering.set(false);
                pressing.set_if_modified(false);
            })
            .child(body);

        let button = {
            let shared = shared.clone();
            focus_stop_press(
                content,
                match column.sort_key() {
                    Some(_) => format!("Sort by {}", column.title()),
                    None => format!("{} column", column.title()),
                },
                AccessibilityRole::Button,
                true,
                Radius::Mark,
                move |_: ()| {
                    let mut ignore = shared.ignore_press;
                    if *ignore.peek() {
                        ignore.set(false);
                        return;
                    }
                    if matches!(*shared.drag.peek(), Some(Drag::Move { .. })) {
                        return;
                    }
                    if let Some(key) = column.sort_key() {
                        shared.on_sort.call(key);
                    }
                },
                pressing,
            )
            .tw(&tokens, &[w_full(), h_full()])
        };

        let edge = (column != Column::Name).then(|| {
            let shared = shared.clone();
            rect()
                // Centred on the cell's left edge, the edge that follows the
                // pointer while Name flexes (see `Drag::Resize`).
                .position(Position::new_absolute().top(0.0).left(-HANDLE_PX / 2.0))
                .width(Size::px(HANDLE_PX))
                // Above the heading: its text sits three layers deeper than the
                // cell, and a deeper node would take the press before the handle.
                .layer(Layer::Relative(HANDLE_LAYER))
                .tw(&tokens, &[items(Align::Center), justify(Align::Center)])
                // `h_full` on an absolute box measures zero; a percent does not.
                .height(Size::percent(100.0))
                .cursor(CursorIcon::ColResize)
                .on_pointer_enter(move |_| edge_hot.set(true))
                .on_pointer_leave(move |_| edge_hot.set(false))
                .on_pointer_down(move |event: Event<PointerEventData>| {
                    if !matches!(
                        event.data(),
                        PointerEventData::Mouse(mouse) if mouse.button == Some(MouseButton::Left)
                    ) {
                        return;
                    }
                    event.stop_propagation();
                    let now = Instant::now();
                    let mut last = shared.last_edge_press;
                    let mut drag = shared.drag;
                    let double = matches!(
                        *last.peek(),
                        Some((at, pressed)) if pressed == column && now.duration_since(at) < DOUBLE_CLICK
                    );
                    if double {
                        last.set(None);
                        drag.set(None);
                        fit_column(&shared, column);
                        return;
                    }
                    last.set(Some((now, column)));
                    drag.set(Some(Drag::Resize {
                        column,
                        start_x: event.global_location().x as f32,
                        start_width: shared.layout.peek().width(column),
                    }));
                })
                .child(rect().tw(
                    &tokens,
                    &[
                        w(Dim::Hairline),
                        h_full(),
                        bg(if resizing {
                            Paint::Accent
                        } else if edge_hot() {
                            Paint::Separator
                        } else {
                            Paint::Transparent
                        }),
                    ],
                ))
        });

        let on_key_down = {
            let shared = shared.clone();
            move |event: Event<KeyboardEventData>| {
                let shift = event.modifiers.contains(Modifiers::SHIFT);
                let ctrl = event.modifiers.contains(Modifiers::CONTROL);
                let key = direction.reading_key(&event.key);
                let step = match &key {
                    Key::Named(NamedKey::ArrowLeft) => -1,
                    Key::Named(NamedKey::ArrowRight) => 1,
                    Key::Named(NamedKey::ContextMenu) => {
                        event.stop_propagation();
                        let below = area.peek().clone();
                        shared.on_menu.call((below.min_x(), below.max_y()));
                        return;
                    }
                    Key::Named(NamedKey::F10) if shift => {
                        event.stop_propagation();
                        let below = area.peek().clone();
                        shared.on_menu.call((below.min_x(), below.max_y()));
                        return;
                    }
                    _ => return,
                };
                if column == Column::Name || shift == ctrl {
                    return;
                }
                event.stop_propagation();
                if shift {
                    let Some(index) = shared.drawn.iter().position(|known| *known == column)
                    else {
                        return;
                    };
                    let slot = (index as i32 + step).clamp(0, shared.drawn.len() as i32 - 1);
                    if slot as usize != index {
                        shared.edit(|layout| {
                            layout.move_to_slot(column, slot as usize, &shared.drawn)
                        });
                        shared.save();
                    }
                } else {
                    let width = shared.layout.peek().width(column) + step as f32 * KEY_RESIZE_STEP;
                    if shared.edit(|layout| layout.resize(column, width)) {
                        shared.save();
                    }
                }
            }
        };

        let mut cell = rect()
            .a11y_role(AccessibilityRole::ColumnHeader)
            .offset_x(self.offset)
            .on_sized(move |event: Event<SizedEventData>| area.set_if_modified(event.area))
            .on_key_down(on_key_down)
            .maybe(self.moving, |cell| cell.layer(Layer::Relative(1)));
        cell = match self.width {
            Some(width) => cell.width(Size::px(width)).tw(&tokens, &[h_full()]),
            None => cell
                .min_width(Size::px(self.name_min))
                .tw(&tokens, &[w_flex(), h_full()]),
        };
        cell.child(button).maybe_child(edge)
    }
}

/// Fit `column` to its widest cell, from the owner's measure. The heading's
/// own padding and sort arrow are added to the measured text.
fn fit_column(shared: &Shared, column: Column) {
    let text = (shared.fit.0)(column);
    let arrow = if column.sort_key().is_some() {
        IconSize::Mark.px() + S1.px()
    } else {
        0.0
    };
    let drawn = text + arrow + 2.0 * CELL_PADDING.px();
    shared.edit(|layout| layout.fit(column, drawn));
    shared.save();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bingux_files::mime;

    use super::*;

    /// 2024-03-09 12:34:56 UTC, a Saturday.
    const THEN: i64 = 1_709_987_696;

    fn at(seconds: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    }

    fn shown(layout: &ColumnLayout) -> Vec<Column> {
        layout.visible(true)
    }

    #[test]
    fn default_layout() {
        let layout = ColumnLayout::default();
        assert_eq!(layout.order(), Column::ALL.to_vec());
        assert_eq!(
            shown(&layout),
            vec![Column::Size, Column::Modified, Column::Location]
        );
        assert_eq!(
            layout.visible(false),
            vec![Column::Size, Column::Modified]
        );
        assert!(layout.is_visible(Column::Name));
        assert!(!layout.is_visible(Column::Kind));
    }

    #[test]
    fn plan_fits() {
        let layout = ColumnLayout::default();
        // Name 160 + Size 88 + Modified 120 = 368.
        let plan = plan(&layout, 400.0, false);
        assert_eq!(plan.hidden, 0);
        assert_eq!(
            plan.columns,
            vec![(Column::Size, 88.0), (Column::Modified, 120.0)]
        );
        assert_eq!(plan.name_min, 160.0);
        assert!(!plan.with_location);
    }

    #[test]
    fn plan_hides_rightmost_first() {
        let layout = ColumnLayout::default();
        // Room for Name and one more column only.
        let plan = plan(&layout, 160.0 + 90.0, true);
        assert_eq!(plan.columns, vec![(Column::Size, 88.0)]);
        assert_eq!(plan.hidden, 2);
        // No room at all: Name is still there, every other column is gone.
        let plan = super::plan(&layout, 100.0, true);
        assert!(plan.columns.is_empty());
        assert_eq!(plan.hidden, 3);
    }

    #[test]
    fn plan_follows_the_layout_order() {
        let mut layout = ColumnLayout::default();
        layout.move_column(Column::Modified, 1);
        let plan = plan(&layout, 160.0 + 130.0, false);
        // Modified is first now, so Size is the one that goes.
        assert_eq!(plan.columns, vec![(Column::Modified, 120.0)]);
        assert_eq!(plan.hidden, 1);
    }

    #[test]
    fn plan_does_not_depend_on_the_zoom() {
        let layout = ColumnLayout::default();
        let plan = plan(&layout, 1000.0, false);
        assert_eq!(plan.name_min, 160.0);
        assert_eq!(
            plan.columns,
            vec![(Column::Size, 88.0), (Column::Modified, 120.0)]
        );
    }

    #[test]
    fn the_icon_slot_grows_with_the_icon() {
        assert_eq!(gutter(32.0) - gutter(16.0), 16.0);
        let layout = ColumnLayout::default();
        let narrow = plan_in(&layout, 16.0, 420.0, false);
        let wide_icon = plan_in(&layout, 32.0, 420.0, false);
        assert_eq!(narrow.columns, wide_icon.columns);
        // A bigger icon takes room from the columns, not the other way.
        let tight = plan_in(&layout, 32.0, 160.0 + 130.0 + gutter(32.0) - 20.0, false);
        assert_eq!(tight.columns, vec![(Column::Size, 88.0)]);
        assert_eq!(tight.hidden, 1);
    }

    #[test]
    fn plan_without_measured_room_keeps_everything() {
        let layout = ColumnLayout::default();
        let plan = plan(&layout, 0.0, true);
        assert_eq!(plan.columns.len(), 3);
        assert_eq!(plan.hidden, 0);
    }

    #[test]
    fn plan_leaves_location_out_without_it() {
        let layout = ColumnLayout::default();
        let plan = plan(&layout, 10_000.0, false);
        assert!(plan.columns.iter().all(|(column, _)| *column != Column::Location));
        let plan = super::plan(&layout, 10_000.0, true);
        assert_eq!(plan.columns.last().map(|(c, _)| *c), Some(Column::Location));
    }

    #[test]
    fn resize_and_fit_are_clamped() {
        let mut layout = ColumnLayout::default();
        layout.resize(Column::Size, 10.0);
        assert_eq!(layout.width(Column::Size), Column::Size.min_width());
        layout.resize(Column::Size, 5_000.0);
        assert_eq!(layout.width(Column::Size), MAX_WIDTH);
        layout.resize(Column::Size, 100.0);
        assert_eq!(layout.width(Column::Size), 100.0);
        layout.fit(Column::Owner, 1.0);
        assert_eq!(layout.width(Column::Owner), Column::Owner.min_width());
        layout.fit(Column::Owner, 130.0);
        assert_eq!(layout.width(Column::Owner), 130.0);
        // Name takes no width of its own.
        layout.resize(Column::Name, 500.0);
        assert_eq!(layout.width(Column::Name), Column::Name.default_width());
        layout.resize(Column::Size, f32::NAN);
        assert_eq!(layout.width(Column::Size), Column::Size.default_width());
    }

    #[test]
    fn name_cannot_move_or_hide() {
        let mut layout = ColumnLayout::default();
        layout.move_column(Column::Name, 4);
        layout.set_visible(Column::Name, false);
        assert_eq!(layout.order()[0], Column::Name);
        assert!(layout.is_visible(Column::Name));
        // Nothing goes before Name.
        layout.move_column(Column::Modified, 0);
        assert_eq!(layout.order()[0], Column::Name);
        assert_eq!(layout.order()[1], Column::Modified);
    }

    #[test]
    fn move_column_to_index() {
        let mut layout = ColumnLayout::default();
        layout.move_column(Column::Owner, 2);
        assert_eq!(
            layout.order()[..4],
            [Column::Name, Column::Size, Column::Owner, Column::Kind]
        );
        layout.move_column(Column::Size, 99);
        assert_eq!(layout.order().last(), Some(&Column::Size));
    }

    #[test]
    fn move_to_slot_among_shown_columns() {
        let mut layout = ColumnLayout::default();
        // Shown: Size, Modified, Location; Kind sits hidden between.
        let among = shown(&layout);
        layout.move_to_slot(Column::Location, 0, &among);
        assert_eq!(shown(&layout), vec![Column::Location, Column::Size, Column::Modified]);
        let among = shown(&layout);
        layout.move_to_slot(Column::Location, 99, &among);
        assert_eq!(shown(&layout), vec![Column::Size, Column::Modified, Column::Location]);
        // A slot of its own index changes nothing.
        let before = layout.clone();
        let among = shown(&layout);
        layout.move_to_slot(Column::Modified, 1, &among);
        assert_eq!(layout, before);
        // One step right swaps with the neighbour.
        let among = shown(&layout);
        layout.move_to_slot(Column::Size, 1, &among);
        assert_eq!(shown(&layout), vec![Column::Modified, Column::Size, Column::Location]);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut layout = ColumnLayout::default();
        layout.resize(Column::Size, 200.0);
        layout.set_visible(Column::Owner, true);
        layout.move_column(Column::Owner, 1);
        layout.reset();
        assert_eq!(layout, ColumnLayout::default());
    }

    #[test]
    fn drop_slot_follows_the_centre() {
        let widths = [100.0, 100.0, 100.0];
        // Not moved, or moved a little: stays.
        assert_eq!(drop_slot(&widths, 0, 0.0), 0);
        assert_eq!(drop_slot(&widths, 0, 40.0), 0);
        // Centre passes the second column's centre (150): second place.
        assert_eq!(drop_slot(&widths, 0, 120.0), 1);
        assert_eq!(drop_slot(&widths, 0, 400.0), 2);
        assert_eq!(drop_slot(&widths, 2, -120.0), 1);
        assert_eq!(drop_slot(&widths, 2, -400.0), 0);
    }

    #[test]
    fn drop_line_sits_between_the_others() {
        let widths = [100.0, 60.0, 40.0];
        // Moving the first: the others start at 100 and 160, end at 200.
        assert_eq!(line_position(&widths, 0, 0), 100.0);
        assert_eq!(line_position(&widths, 0, 1), 160.0);
        assert_eq!(line_position(&widths, 0, 2), 200.0);
        // Moving the last, to the end: after the middle column.
        assert_eq!(line_position(&widths, 2, 2), 160.0);
        assert_eq!(line_position(&widths, 2, 0), 0.0);
    }

    #[test]
    fn saved_layout_round_trips() {
        let mut layout = ColumnLayout::default();
        layout.resize(Column::Size, 130.0);
        layout.set_visible(Column::Kind, true);
        layout.move_column(Column::Location, 1);
        let json = serde_json::to_string(&layout).unwrap();
        let back: ColumnLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, layout);
    }

    #[test]
    fn saved_layout_is_repaired() {
        let json = r#"{"columns":[
            {"column":"owner","width":70.0,"visible":true},
            {"column":"sparkle","width":10.0,"visible":true},
            {"column":"name","width":10.0,"visible":false},
            {"column":"size","width":1.0,"visible":true},
            {"column":"size","width":300.0,"visible":false},
            {"column":"modified","width":99999.0,"visible":false}
        ]}"#;
        let layout: ColumnLayout = serde_json::from_str(json).unwrap();
        let order = layout.order();
        // Known columns keep their saved order, new ones follow.
        assert_eq!(order[..4], [Column::Name, Column::Owner, Column::Size, Column::Modified]);
        assert_eq!(order.len(), Column::ALL.len());
        assert!(layout.is_visible(Column::Owner));
        // The unknown column is dropped, the first duplicate wins.
        assert_eq!(layout.width(Column::Size), Column::Size.min_width());
        assert!(layout.is_visible(Column::Size));
        assert_eq!(layout.width(Column::Modified), MAX_WIDTH);
        assert!(!layout.is_visible(Column::Modified));
        // Columns the file lacked start as a fresh layout has them.
        assert!(layout.is_visible(Column::Location));
        assert!(!layout.is_visible(Column::Kind));
        assert_eq!(layout.width(Column::Kind), Column::Kind.default_width());
    }

    #[test]
    fn empty_saved_layout_is_the_default() {
        let layout: ColumnLayout = serde_json::from_str("{}").unwrap();
        assert_eq!(layout, ColumnLayout::default());
    }

    fn file(name: &str, size: u64, modified: Option<i64>) -> Entry {
        let base = Entry::from_path(Path::new("/"), false).unwrap();
        Entry {
            name: name.to_string(),
            path: PathBuf::from("/home/kieran/Documents/Reports").join(name),
            kind: EntryKind::File,
            size,
            modified: modified.map(at),
            hidden: false,
            symlink: false,
            mime: mime::for_name(name),
            ..base
        }
    }

    fn text(column: Column, entry: &Entry, now: i64, dates: DateStyle) -> String {
        text_with(
            column,
            entry,
            at(now),
            Path::new("/home/kieran"),
            dates,
            &|_| 0,
        )
    }

    #[test]
    fn name_size_and_kind() {
        let entry = file("report.pdf", 1_500, None);
        let now = THEN;
        assert_eq!(text(Column::Name, &entry, now, DateStyle::Absolute), "report.pdf");
        assert_eq!(text(Column::Size, &entry, now, DateStyle::Absolute), "1.5 kB");
        let kind = text(Column::Kind, &entry, now, DateStyle::Absolute);
        assert!(!kind.is_empty());
        assert_eq!(kind, entry.mime.description);
        // A folder shows no size.
        let folder = Entry {
            kind: EntryKind::Directory,
            ..file("Reports", 0, None)
        };
        assert_eq!(text(Column::Size, &folder, now, DateStyle::Absolute), "");
    }

    #[test]
    fn absolute_dates() {
        let entry = file("a.txt", 1, Some(THEN));
        // The same year drops the year.
        assert_eq!(
            text(Column::Modified, &entry, THEN + 40 * 86_400, DateStyle::Absolute),
            "9 Mar"
        );
        // Another year keeps it.
        assert_eq!(
            text(Column::Modified, &entry, THEN + 400 * 86_400, DateStyle::Absolute),
            "9 Mar 2024"
        );
        // The offset moves a time across midnight: 22:34 UTC is the 10th at +2 h.
        assert_eq!(absolute_date(THEN + 10 * 3600, THEN + 3600, 7200), "10 Mar");
        // No time, no text.
        let none = file("b.txt", 1, None);
        assert_eq!(text(Column::Modified, &none, THEN, DateStyle::Absolute), "");
        assert_eq!(text(Column::Created, &entry, THEN, DateStyle::Absolute), "");
    }

    #[test]
    fn relative_dates() {
        let entry = file("a.txt", 1, Some(THEN));
        let relative = |ago: i64| text(Column::Modified, &entry, THEN + ago, DateStyle::Relative);
        assert_eq!(relative(0), "Just now");
        assert_eq!(relative(59), "Just now");
        assert_eq!(relative(60), "1 minute ago");
        assert_eq!(relative(12 * 60), "12 minutes ago");
        assert_eq!(relative(3600), "1 hour ago");
        assert_eq!(relative(3 * 3600 + 20), "3 hours ago");
        // 12:34 plus seven hours is still the same day.
        assert_eq!(relative(7 * 3600), "Today 12:34");
        assert_eq!(relative(86_400), "Yesterday 12:34");
        assert_eq!(relative(3 * 86_400), "Sat 12:34");
        assert_eq!(relative(6 * 86_400), "Sat 12:34");
        assert_eq!(relative(7 * 86_400), "9 Mar");
        assert_eq!(relative(400 * 86_400), "9 Mar 2024");
        // A time a few seconds ahead of the clock is still just now.
        assert_eq!(
            text(Column::Modified, &entry, THEN - 20, DateStyle::Relative),
            "Just now"
        );
    }

    #[test]
    fn relative_dates_across_midnight_count_hours_first() {
        // 23:30 and an hour later, at 00:30 the next day.
        let late = THEN + (23 * 3600 + 30 * 60) - (12 * 3600 + 34 * 60 + 56);
        assert_eq!(relative_date(late, late + 3600, 0), "1 hour ago");
        assert_eq!(relative_date(late, late + 7 * 3600, 0), "Yesterday 23:30");
    }

    #[test]
    fn permissions() {
        assert_eq!(permissions_text(0o755, EntryKind::Directory, false), "drwxr-xr-x");
        assert_eq!(permissions_text(0o644, EntryKind::File, false), "-rw-r--r--");
        assert_eq!(permissions_text(0o777, EntryKind::File, true), "lrwxrwxrwx");
        assert_eq!(permissions_text(0o000, EntryKind::Other, false), "?---------");
        assert_eq!(permissions_text(0o4755, EntryKind::File, false), "-rwsr-xr-x");
        assert_eq!(permissions_text(0o4644, EntryKind::File, false), "-rwSr--r--");
        assert_eq!(permissions_text(0o1777, EntryKind::Directory, false), "drwxrwxrwt");
        assert_eq!(permissions_text(0o1776, EntryKind::Directory, false), "drwxrwxrwT");
    }

    #[test]
    fn locations() {
        let home = Path::new("/home/kieran");
        assert_eq!(
            location_text(Path::new("/home/kieran/Documents/Reports/a.pdf"), home),
            "~/Documents/Reports"
        );
        assert_eq!(location_text(Path::new("/home/kieran/a.pdf"), home), "~");
        assert_eq!(location_text(Path::new("/srv/share/a.pdf"), home), "/srv/share");
        // A sibling that merely starts with the same letters is not inside it.
        assert_eq!(
            location_text(Path::new("/home/kieran2/a.pdf"), home),
            "/home/kieran2"
        );
        let entry = file("a.pdf", 1, None);
        assert_eq!(
            text(Column::Location, &entry, THEN, DateStyle::Absolute),
            "~/Documents/Reports"
        );
    }
}
