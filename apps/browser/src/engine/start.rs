//! Starting the engine.

use super::Engine;
use crate::profiles::places::Folders;

impl Engine {
    /// Start the engine in `folders`, or in a throwaway profile for a private window.
    #[cfg(not(any(feature = "vivarium", feature = "dynamic-load")))]
    pub fn start(_folders: Option<&Folders>) -> Result<Engine, String> {
        Err(
            "this build has no engine. Build it with the `vivarium` feature, against a Vivarium SDK."
                .to_string(),
        )
    }

    #[cfg(any(feature = "vivarium", feature = "dynamic-load"))]
    pub fn start(folders: Option<&Folders>) -> Result<Engine, String> {
        use bingux_ui::web_view::engine::{Config, EngineFolders};
        let folders = match folders {
            None => EngineFolders::Private,
            Some(folders) => EngineFolders::Profile {
                profile: folders.engine.clone(),
                cache: folders.cache.clone(),
                crash: folders.crash.clone(),
                runtime: folders.runtime.clone(),
            },
        };
        // An engine that does not start is not a reason to exit: the window opens and says why
        // (a profile folder that is not safe, a sandbox it cannot reach), and offers to try again.
        Ok(Engine::new(bingux_ui::web_view::engine::start(
            Config::from_environment(folders),
        )))
    }
}
