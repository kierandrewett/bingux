//! A window with a frame view showing the bundled test pattern, and a line
//! that says what the producer last heard.
//!
//! ```sh
//! cargo run --example frame_view
//! ```
//!
//! The pattern is drawn by a thread of its own at 60 frames a second, at the
//! size the view reports, so on any display scale it lands pixel for pixel.
//! Press, drag, scroll or type over it; Tab leaves the view and F6 cycles the
//! window's regions. A real producer replaces `PatternSource` with its own
//! `FrameSource`; nothing else changes.

use bingux_ui::frame_view::{
    KeyInput, PatternSource, PointerInput, ViewSize, WheelInput, frame_view,
};
use bingux_ui::prelude::*;
use bingux_ui::theme::{SharedTokens, Tokens, freya_theme, load_tokens};
use freya::prelude::*;

fn main() {
    // The tokens come from the desktop's settings, read before the window exists.
    let tokens = load_tokens();
    Application::new("bingux-frame-view")
        .title("Frame view")
        .size(720.0, 520.0)
        .run(move || Root(tokens));
}

/// The window root: it provides the tokens the window host reads.
#[derive(PartialEq)]
struct Root(Tokens);

impl Component for Root {
    fn render(&self) -> impl IntoElement {
        let tokens = self.0;
        let shared = use_hook(move || SharedTokens(State::create(tokens)));
        use_provide_root_context(move || shared);
        let tokens = *shared.0.read();
        let _ = use_init_theme(|| freya_theme(&tokens));
        WindowHost::new(OverlayHost::new(Demo))
    }
}

#[derive(PartialEq)]
struct Demo;

impl Component for Demo {
    fn render(&self) -> impl IntoElement {
        let tokens = use_tokens();
        let mut heard = use_state(|| "Move over the view, press, scroll or type.".to_string());
        // The producer: a thread that draws a picture and wakes the view.
        let source = use_hook(|| PatternSource::animated(640, 360, 60));
        let follower = source.clone();
        let view = frame_view(source, "Test pattern")
            .placeholder("Starting")
            // Render at the size the view really has: 1:1 on every display.
            .on_resize(move |size: ViewSize| {
                let (width, height) = size.device_size();
                follower.resize(width, height, size.scale as f32);
            })
            .on_pointer(move |input: PointerInput| {
                heard.set(format!(
                    "{:?} at {:.0}, {:.0}",
                    input.action, input.position.0, input.position.1
                ));
            })
            .on_wheel(move |input: WheelInput| heard.set(format!("wheel {:?}", input.delta)))
            .on_key(move |input: KeyInput| {
                heard.set(format!(
                    "key {} ({})",
                    input.key_string(),
                    input.code_string()
                ))
            });
        rect()
            .tw(
                &tokens,
                &[
                    w_full(),
                    h_full(),
                    flex_col(),
                    grow_children(),
                    p(S4),
                    gap(S3),
                ],
            )
            .child(rect().tw(&tokens, &[w_full(), h_flex()]).child(view))
            .child(caption(heard.read().clone()))
    }
}
