//! The engine's life as the window sees it: it fails, and a restart opens the tabs again.

use bingux_ui::web_view::FailureCause;

use super::Status;
use crate::browser::Browser;
use crate::tabs::TabKind;

impl Browser {
    /// The engine is gone: every session with it. The tabs stay as they were,
    /// without a session, so a restart can open them again; what each was
    /// waiting for will never be answered by anyone.
    pub(crate) fn engine_failed(
        &mut self,
        message: String,
        code: u32,
        cause: FailureCause,
        restartable: bool,
    ) {
        for tab in self.tabs.all_mut() {
            let Some(session) = tab.session.take() else {
                continue;
            };
            self.lost.push((tab.key, session));
            tab.ready = false;
            tab.load.loading = false;
            tab.audio.playing = false;
            tab.hover = None;
            tab.prompts.clear();
            tab.cert = None;
            // The toolbox session went with the engine.
            tab.toolbox = None;
        }
        self.menu = None;
        self.engine.status = Status::Failed {
            message,
            code,
            cause,
            restartable,
        };
    }

    /// The engine is up (again): it takes the window's look, and what the
    /// controls may offer is what it says it can do.
    pub(crate) fn engine_ready(&mut self) {
        self.engine.status = Status::Ready;
        self.engine.refresh();
        let environment = self.environment.clone();
        self.engine.call(&mut self.out, |backend| {
            backend.set_environment(&environment)
        });
    }

    /// Start a new engine on the same profile and open every tab again where
    /// it was. Only after a failure that says it is safe. When the engine
    /// still does not start the window keeps saying why (the backend says it
    /// again, with the new words).
    pub fn restart_engine(&mut self) {
        let Status::Failed {
            restartable: true, ..
        } = self.engine.status
        else {
            return;
        };
        if let Err(error) = self.engine.backend().restart() {
            self.out.fail(&error);
            return;
        }
        self.engine.status = Status::Starting;
        self.engine.refresh();
        let active = self.tabs.active_key();
        for (key, old) in std::mem::take(&mut self.lost) {
            let Some(tab) = self.tabs.get(key).filter(|tab| tab.kind == TabKind::Web) else {
                continue;
            };
            let mut config = self.config(&tab.url.clone(), None, key != active);
            config.restore_from = Some(old);
            match self.engine.backend().create_tab(&config) {
                Ok(session) => {
                    if let Some(tab) = self.tabs.get_mut(key) {
                        tab.session = Some(session);
                        tab.gone = None;
                        tab.load.loading = !tab.url.is_empty();
                        tab.load.progress = 0.0;
                        tab.load.error = None;
                    }
                }
                Err(error) => self.out.fail(&error),
            }
        }
        self.shown_again(active);
        if let Some(first) = self.start.take() {
            self.open_first(&first);
        }
    }
}
