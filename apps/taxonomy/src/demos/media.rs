//! Media: the parts a player is made of.

use bingux_ui::media::{
    ArtworkKind, RepeatMode, Seek, TransportAction, VideoFrame, VideoFrames, VolumeChange, artwork,
    format_time, now_playing_bar, timeline, transport, video_surface, volume_control,
};
use bingux_ui::prelude::*;
use bingux_ui::typography::NUMERIC;
use freya::prelude::*;

use crate::model::Spec;

/// The specimens' widths: a small film, a seek bar, a player window.
const VIDEO_WIDTH: f32 = 320.0;
const VIDEO_HEIGHT: f32 = 180.0;
const BAR_WIDTH: f32 = 420.0;
const PLAYER_WIDTH: f32 = 900.0;

/// Colour bars, as a decoder would hand them over.
fn test_pattern(width: u32, height: u32) -> VideoFrame {
    const BARS: [[u8; 3]; 7] = [
        [192, 192, 192],
        [192, 192, 0],
        [0, 192, 192],
        [0, 192, 0],
        [192, 0, 192],
        [192, 0, 0],
        [0, 0, 192],
    ];
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for _ in 0..height {
        for x in 0..width {
            let bar = BARS[(x as usize * BARS.len()) / width as usize];
            pixels.extend_from_slice(&[bar[0], bar[1], bar[2], 255]);
        }
    }
    VideoFrame {
        width,
        height,
        pixels: pixels.into(),
    }
}

pub const VIDEO_SURFACE: &[Spec] = &[Spec::live("Frames pushed from another thread", || {
    let frames = use_hook(|| {
        let frames = VideoFrames::new();
        frames.push(test_pattern(320, 180));
        frames
    });
    rect()
        .width(Size::px(VIDEO_WIDTH))
        .height(Size::px(VIDEO_HEIGHT))
        .child(video_surface(frames).label("Colour bars"))
        .into_element()
})];

pub const TIMELINE: &[Spec] = &[
    Spec::live("Playing, with a buffered stretch", || {
        let mut at = use_state(|| 83.0);
        rect()
            .width(Size::px(BAR_WIDTH))
            .child(
                timeline(*at.read(), Some(245.0), move |seek: Seek| at.set(seek.to))
                    .buffered(vec![(0.0, 140.0)])
                    .times(),
            )
            .into_element()
    }),
    Spec::live("Chapters and an A-B loop", || {
        let mut at = use_state(|| 640.0);
        rect()
            .width(Size::px(BAR_WIDTH))
            .child(
                timeline(*at.read(), Some(1800.0), move |seek: Seek| at.set(seek.to))
                    .chapters(vec![
                        (0.0, "Opening".into()),
                        (600.0, "The middle".into()),
                        (1200.0, "Ending".into()),
                    ])
                    .loop_points(Some(300.0), Some(900.0))
                    .times(),
            )
            .into_element()
    }),
    Spec::forced("Not loaded", || {
        rect()
            .width(Size::px(BAR_WIDTH))
            .child(timeline(0.0, None, |_: Seek| {}).times())
            .into_element()
    }),
];

pub const TRANSPORT: &[Spec] = &[
    Spec::live("Music: shuffle and repeat", || {
        let mut playing = use_state(|| true);
        let mut shuffle = use_state(|| false);
        let mut repeat = use_state(|| RepeatMode::Off);
        transport(
            *playing.read(),
            move |action: TransportAction| match action {
                TransportAction::PlayPause => {
                    let now = *playing.peek();
                    playing.set(!now)
                }
                TransportAction::Shuffle => {
                    let now = *shuffle.peek();
                    shuffle.set(!now)
                }
                TransportAction::Repeat => {
                    let next = match *repeat.read() {
                        RepeatMode::Off => RepeatMode::All,
                        RepeatMode::All => RepeatMode::One,
                        RepeatMode::One => RepeatMode::Off,
                    };
                    repeat.set(next);
                }
                _ => {}
            },
        )
        .shuffle(*shuffle.read())
        .repeat(*repeat.read())
        .into_element()
    }),
    Spec::forced("A film: back and forward 10 seconds", || {
        transport(false, |_: TransportAction| {})
            .skips()
            .can_next(false)
            .into_element()
    }),
    Spec::forced("Nothing to play", || {
        transport(false, |_: TransportAction| {})
            .enabled(false)
            .into_element()
    }),
];

pub const VOLUME: &[Spec] = &[
    Spec::live("Level and mute; the wheel steps by 5", || {
        let mut level = use_state(|| 70.0);
        let mut muted = use_state(|| false);
        volume_control(
            *level.read(),
            *muted.read(),
            move |change: VolumeChange| match change {
                VolumeChange::Level(value) => {
                    level.set(value);
                    muted.set(false);
                }
                VolumeChange::Mute(on) => muted.set(on),
            },
        )
        .into_element()
    }),
    Spec::forced("Muted, button alone", || {
        volume_control(70.0, true, |_: VolumeChange| {})
            .compact()
            .into_element()
    }),
];

pub const ARTWORK: &[Spec] = &[
    Spec::forced("Fallbacks made from the name", || {
        hstack(S3)
            .child(artwork(None, "Northern Lights").kind(ArtworkKind::Album))
            .child(artwork(None, "Aurora Lane").kind(ArtworkKind::Artist))
            .child(
                artwork(None, "First Light")
                    .kind(ArtworkKind::Song)
                    .size(Dim::Box11),
            )
            .child(
                artwork(None, "Beach Day")
                    .kind(ArtworkKind::Video)
                    .side(170.0),
            )
            .into_element()
    }),
    Spec::forced("A missing picture falls back", || {
        artwork(Some("/nonexistent/cover.jpg".into()), "Sine Waves").into_element()
    }),
    Spec::forced("Clock readings", || {
        let tokens = use_tokens();
        hstack(S3)
            .child(NUMERIC.label(&tokens, format_time(7.0)))
            .child(NUMERIC.label(&tokens, format_time(225.0)))
            .child(NUMERIC.label(&tokens, format_time(3723.0)))
            .child(NUMERIC.label(&tokens, format_time(-42.0)))
            .into_element()
    }),
];

pub const NOW_PLAYING: &[Spec] = &[Spec::live("Playing a song", || {
    let mut playing = use_state(|| true);
    let mut at = use_state(|| 61.0);
    rect()
        .width(Size::px(PLAYER_WIDTH))
        .child(
            now_playing_bar("First Light", "Aurora Lane, Northern Lights")
                .artwork(None, "Northern Lights", ArtworkKind::Album)
                .playing(*playing.read())
                .progress(*at.read(), Some(240.0))
                .shuffle(false)
                .repeat(RepeatMode::Off)
                .volume(80.0, false, |_: VolumeChange| {})
                .on_transport(move |action: TransportAction| {
                    if action == TransportAction::PlayPause {
                        {
                            let now = *playing.peek();
                            playing.set(!now)
                        };
                    }
                })
                .on_seek(move |seek: Seek| at.set(seek.to)),
        )
        .into_element()
})
.wide()];
