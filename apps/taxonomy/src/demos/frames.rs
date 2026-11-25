//! Controls: the frame view and its test producer, and the web view control
//! over the in-memory engine double.

use std::sync::Arc;

use bingux_ui::frame_view::{
    FrameFit, FrameView, KeyInput, PatternSource, PointerAction, PointerInput, TextInput, ViewSize,
    WheelInput, frame_view,
};
use bingux_ui::prelude::*;
use bingux_ui::web_view::test_double::TestDoubleBackend;
use bingux_ui::web_view::{BrowserBackend, TabConfig, web_view};
use freya::prelude::*;

use crate::model::Spec;

/// A web view of `url`, as an app embeds one: the engine is the in-memory
/// double, which draws a wireframe of the page at the view's size.
fn web_page(url: &'static str, label: &'static str) -> Element {
    let (backend, tab, surface) = use_hook(move || {
        let double = TestDoubleBackend::new();
        let tab = double.create_tab(&TabConfig::default()).expect("the double opens a session");
        let _ = double.load(tab, url);
        let surface = double.surface(tab).expect("a session has a surface");
        (double as Arc<dyn BrowserBackend>, tab, surface)
    });
    rect()
        .width(Size::fill())
        .height(Size::px(VIEW_HEIGHT))
        .child(web_view(backend, tab, surface).label(label))
        .into_element()
}

pub const WEB_VIEW: &[Spec] = &[
    Spec::live("A page in an app: drawn by the engine, driven like a control", || {
        web_page("https://example.com", "Example Domain")
    }),
    Spec::live("The host takes its own keys first (Ctrl+T never reaches the page)", || {
        let mut taken = use_state(|| "Press Ctrl+T over the page".to_string());
        let (backend, tab, surface) = use_hook(|| {
            let double = TestDoubleBackend::new();
            let tab = double.create_tab(&TabConfig::default()).expect("the double opens a session");
            let _ = double.load(tab, "https://example.com");
            let surface = double.surface(tab).expect("a session has a surface");
            (double as Arc<dyn BrowserBackend>, tab, surface)
        });
        let view = web_view(backend, tab, surface).label("Page with host keys").on_host_key(move |input: KeyInput| {
            let mine = input.modifiers.contains(Modifiers::CONTROL) && input.key_string() == "t";
            if mine {
                taken.set("The app took Ctrl+T".to_string());
            }
            mine
        });
        vstack(S3)
            .child(rect().width(Size::fill()).height(Size::px(VIEW_HEIGHT)).child(view))
            .child(caption(taken.read().clone()))
            .into_element()
    }),
];

/// The height of a view in a stage; its width is the stage's.
const VIEW_HEIGHT: f32 = 180.0;
/// The height of each of the two views that show how a frame of another size sits.
const SMALL_HEIGHT: f32 = 120.0;
/// What the animated producer draws before it is told the view's size.
const START_WIDTH: u32 = 320;
const START_HEIGHT: u32 = 180;
const FPS: u32 = 60;

/// A view a fixed height tall that takes the stage's width.
fn framed(view: FrameView, height: f32) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::px(height))
        .child(view)
}

/// An animated pattern that follows the view: it is told the size on every
/// layout and draws at it, so the picture is a 1:1 copy at any display scale.
fn animated(label: &'static str) -> FrameView {
    let source = use_hook(|| PatternSource::animated(START_WIDTH, START_HEIGHT, FPS));
    let follower = source.clone();
    frame_view(source, label)
        .placeholder("Starting")
        .on_resize(move |size: ViewSize| {
            let (width, height) = size.device_size();
            follower.resize(width, height, size.scale as f32);
        })
}

/// A producer that has drawn one frame at a fixed size.
fn still(width: u32, height: u32) -> Arc<PatternSource> {
    use_hook(|| {
        let source = PatternSource::stepped(width, height);
        source.step();
        source
    })
}

fn describe(input: &PointerInput) -> String {
    let action = match input.action {
        PointerAction::Move => "move",
        PointerAction::Down => "down",
        PointerAction::Up => "up",
        PointerAction::Enter => "enter",
        PointerAction::Leave => "leave",
        _ => "pointer",
    };
    let clicks = if input.click_count > 1 {
        format!(" x{}", input.click_count)
    } else {
        String::new()
    };
    format!(
        "{action} at {:.0}, {:.0}{clicks}",
        input.position.0, input.position.1
    )
}

/// A view that grows while its producer lags: the producer answers only when
/// told to, so the screenshot before the answer is what a user sees while the
/// engine is still resizing.
#[derive(PartialEq)]
struct ResizeDemo;

impl Component for ResizeDemo {
    fn render(&self) -> impl IntoElement {
        let source = use_hook(|| {
            let source = PatternSource::stepped(START_WIDTH / 2, SMALL_HEIGHT as u32);
            source.step();
            source
        });
        let mut grown = use_state(|| false);
        let answer = source.clone();
        let (width, height) = if grown() {
            (START_WIDTH as f32, VIEW_HEIGHT)
        } else {
            (START_WIDTH as f32 / 2.0, SMALL_HEIGHT)
        };
        vstack(S3)
            .child(
                rect()
                    .width(Size::px(width))
                    .height(Size::px(height))
                    .child(frame_view(source, "Frame that lags a resize")),
            )
            .child(
                vstack(S2)
                    .child(action_button("Grow the view", move |_| grown.set(true)).compact())
                    .child(
                        action_button("Producer answers", move |_| {
                            answer.resize(START_WIDTH, VIEW_HEIGHT as u32, 1.0);
                            answer.step();
                        })
                        .compact(),
                    ),
            )
    }
}

pub const FRAME_VIEW: &[Spec] = &[
    Spec::live("Animated pattern, rendered at the view's size", || {
        framed(animated("Animated test pattern"), VIEW_HEIGHT).into_element()
    }),
    Spec::live("A resize: the old frame stays 1:1 until the producer answers", || {
        ResizeDemo.into_element()
    }),
    Spec::live("After the view grew and before the producer answered", || {
        ResizeDemo.into_element()
    })
    .script("click-text Grow the view; wait 400; save {out}"),
    Spec::live("After the producer answered", || ResizeDemo.into_element())
        .script("click-text Grow the view; wait 300; click-text Producer answers; wait 500; save {out}"),
    // The view is a Tab stop like any control; a click gives it focus and the
    // next key (here an arrow, which the producer takes) puts the window in
    // keyboard mode, so the ring shows.
    Spec::live("Keyboard focus: the ring around the view", || {
        framed(
            animated("Test pattern with the keyboard focus"),
            VIEW_HEIGHT,
        )
        .into_element()
    })
    .script("click-name Test pattern with the keyboard focus; key left; wait 300; save {out}"),
    Spec::forced("No frame yet", || {
        // A producer that has drawn nothing: the view shows its placeholder.
        let source = use_hook(|| PatternSource::stepped(START_WIDTH, START_HEIGHT));
        framed(
            frame_view(source, "Waiting for the first frame")
                .placeholder("Waiting for the first frame"),
            VIEW_HEIGHT,
        )
        .into_element()
    }),
    Spec::forced("A frame of another size: anchored, then contained", || {
        // A source feeds one view, so each has its own.
        let corner = still(160, 90);
        let centred = still(160, 90);
        vstack(S3)
            .child(framed(
                frame_view(corner, "Frame at its own size").fit(FrameFit::Anchored),
                SMALL_HEIGHT,
            ))
            .child(framed(
                frame_view(centred, "Frame contained").fit(FrameFit::Contain),
                SMALL_HEIGHT,
            ))
            .into_element()
    }),
    Spec::live(
        "Input reaches the producer in the view's own pixels",
        || {
            let mut last = use_state(|| "Move, press, scroll or type over the view".to_string());
            let source = still(START_WIDTH, START_HEIGHT);
            let view = frame_view(source, "Frame that reports its input")
                .on_pointer(move |input: PointerInput| last.set(describe(&input)))
                .on_wheel(move |input: WheelInput| last.set(format!("wheel {:?}", input.delta)))
                .on_key(move |input: KeyInput| {
                    let state = if input.pressed { "down" } else { "up" };
                    let repeat = if input.repeat { " (repeat)" } else { "" };
                    last.set(format!(
                        "key {state} {} / {}{repeat}",
                        input.key_string(),
                        input.code_string()
                    ));
                })
                .on_text(move |input: TextInput| last.set(format!("text {input:?}")));
            vstack(S3)
                .child(framed(view, VIEW_HEIGHT))
                .child(caption(last.read().clone()))
                .into_element()
        },
    ),
];
