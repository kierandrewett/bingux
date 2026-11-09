//! Music and films: the parts a player is made of.
//!
//! None of these plays anything. A player (`bingux-media`, which drives mpv,
//! or any other decoder) owns the sound and the picture; these draw it and
//! hand back what the person asked for.
//!
//! | Part | What it is |
//! | ---- | ---------- |
//! | [`video_surface`] with [`VideoFrames`] | The picture: frames from a decoder's thread, drawn at the size the decoder rendered them |
//! | [`timeline`] | The seek bar: buffered stretches, chapter gaps, A-B points, a hover bubble with the time and a picture |
//! | [`transport`] | Shuffle, previous, play or pause, next, repeat (or back and forward 10 s) |
//! | [`volume_control`] | The speaker button and the level; the wheel works on both |
//! | [`artwork`] | Cover art or a film's frame, with a fallback made from the name |
//! | [`now_playing_bar`] | All of them along the bottom of a window, and a phone's mini player |
//! | [`format_time`] | `3:45`, `1:02:03`, `-0:12` |

mod artwork;
mod frames;
mod now_playing;
mod surface;
mod time;
mod timeline;
mod transport;
mod volume;

pub use artwork::{Artwork, ArtworkKind, artwork};
pub use frames::{VideoFrame, VideoFrames};
pub use now_playing::{NowPlayingBar, now_playing_bar};
pub use surface::{VideoSurface, video_surface};
pub use time::format_time;
pub use timeline::{Seek, Timeline, timeline};
pub use transport::{RepeatMode, Transport, TransportAction, transport};
pub use volume::{VolumeChange, VolumeControl, volume_control};
