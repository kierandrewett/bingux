//! Where the engine is.

use bingux_ui::web_view::FailureCause;

/// Where the engine is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Ready,
    /// The engine is gone, and every session with it. The tabs stay (without a
    /// session) until a restart opens them again, or the window is closed.
    Failed {
        message: String,
        /// The engine's exit code, or 128 plus the signal; 0 when it never started.
        code: u32,
        cause: FailureCause,
        /// A new engine on the same profile may be started (`Browser::restart_engine`).
        restartable: bool,
    },
}
