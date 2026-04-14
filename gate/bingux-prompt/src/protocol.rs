/// D-Bus interface name for the Bingux permission prompt service.
///
/// This interface is implemented by the prompt daemon and called by
/// `bingux-gated` when a sandboxed process requests a gated resource.
pub const DBUS_INTERFACE: &str = "org.bingux.Prompt";

/// D-Bus object path for the prompt service.
pub const DBUS_OBJECT_PATH: &str = "/org/bingux/Prompt";

/// Well-known D-Bus bus name for the prompt service.
pub const DBUS_BUS_NAME: &str = "org.bingux.Prompt";

/// Method: ShowPrompt
///
/// Input: `PromptRequest` serialized as a JSON string.
/// Output: `PromptResponse` serialized as a JSON string.
///
/// Called by bingux-gated to display a permission prompt to the user and
/// block until they respond (or the prompt is dismissed).
pub const METHOD_SHOW_PROMPT: &str = "ShowPrompt";

/// Method: Dismiss
///
/// Input: `prompt_id` (`u64`)
///
/// Dismisses a pending prompt, e.g. when the requesting process exits
/// before the user has responded.
pub const METHOD_DISMISS: &str = "Dismiss";

/// Signal: PromptShown
///
/// Emitted when a permission prompt is displayed to the user.
pub const SIGNAL_PROMPT_SHOWN: &str = "PromptShown";

/// Signal: PromptResponded
///
/// Emitted when the user responds to a permission prompt.
pub const SIGNAL_PROMPT_RESPONDED: &str = "PromptResponded";
