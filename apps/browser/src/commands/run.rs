//! What each command does to the browser: one call to the feature that owns it.

use bingux_ui::prelude::tr;

use super::Command;
use crate::browser::{Browser, Notice, UiRequest};
use crate::tabs::Place;
use crate::tabs::TabKind;
use crate::zoom::Zoom;

impl Browser {
    /// Run a shortcut or a menu command on the tab on show.
    pub fn run(&mut self, command: Command) {
        let key = self.tabs.active_key();
        match command {
            Command::NewTab => {
                self.open_tab("", Place::AfterActive, true);
                self.out.request(UiRequest::FocusAddress);
            }
            Command::NewPrivateWindow => self.out.request(UiRequest::NewPrivateWindow),
            Command::CloseTab => self.close_tab(key),
            Command::ReopenTab => self.reopen_closed(),
            Command::NextTab => self.step(1),
            Command::PreviousTab => self.step(-1),
            Command::GoToTab(n) => {
                if let Some(target) = self.tabs.nth(n) {
                    self.select(target);
                }
            }
            Command::LastTab => {
                if let Some(target) = self.tabs.last() {
                    self.select(target);
                }
            }
            Command::MoveTabLeft => self.shift_tab(key, -1),
            Command::MoveTabRight => self.shift_tab(key, 1),
            Command::FocusAddress => self.out.request(UiRequest::FocusAddress),
            Command::Find => self.open_find(key),
            Command::FindNext => self.find_step(key, true),
            Command::FindPrevious => self.find_step(key, false),
            Command::Reload => self.reload(key, false),
            Command::ReloadBypassCache => self.reload(key, true),
            Command::Back => self.back(key),
            Command::Forward => self.forward(key),
            Command::ZoomIn => self.zoom_by(key, Zoom::stepped_in),
            Command::ZoomOut => self.zoom_by(key, Zoom::stepped_out),
            Command::ZoomReset => self.set_zoom(key, Zoom::DEFAULT),
            Command::Fullscreen => {
                let on = !self.fullscreen;
                self.out.request(UiRequest::Fullscreen(on));
                self.fullscreen = on;
                if let Some(session) = self.session_of(key) {
                    self.engine
                        .call(&mut self.out, |backend| backend.set_fullscreen(session, on));
                }
            }
            Command::Downloads => self.out.request(UiRequest::OpenDownloads),
            Command::Settings => {
                self.open_settings();
            }
            Command::PageInfo => self.out.request(UiRequest::OpenPageInfo),
            Command::ViewSource => {
                let url = self
                    .tabs
                    .active()
                    .filter(|tab| tab.kind == TabKind::Web && !tab.is_blank())
                    .map(|tab| tab.url.clone());
                if let Some(url) = url.filter(|url| !url.starts_with("view-source:")) {
                    self.open_tab(&format!("view-source:{url}"), Place::AfterActive, true);
                }
            }
            Command::Print => self.out.notice(Notice {
                error: true,
                text: tr!("browser-commands-print-unavailable"),
                action: None,
            }),
            Command::Edit(edit) => self.send_edit(key, edit),
        }
    }
}
