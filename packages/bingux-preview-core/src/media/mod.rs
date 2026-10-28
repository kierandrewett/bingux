//! What the header of a picture, a song or a film says about it, without
//! decoding any of it: a picture's size and camera, a recording's length and
//! codec.
//!
//! Each reader opens the file, reads a few kilobytes at most (an MP4 or a
//! Matroska file may need its index, up to [`INDEX_LIMIT`]), and gives up with
//! `None` on anything it does not recognise. They block on the disk: call
//! them from a worker.
//!
//! | Format | [`image_info`] | [`av_info`] |
//! | ------ | -------------- | ----------- |
//! | PNG, GIF, BMP, WebP, JPEG | size; JPEG also its EXIF | |
//! | WAV, FLAC, Ogg Vorbis and Opus | | length, codec |
//! | MP4, M4A, MOV | | length, codecs |
//! | Matroska, WebM | | length, codecs |
//!
//! MP3 length needs a scan of every frame, and SVG, HEIF, AVIF, TIFF and
//! JPEG XL need a container parser each; they show no facts here.

mod av;
mod exif;
mod image;

pub use av::{AvInfo, av_info, av_info_from};
pub use exif::{Exif, parse_exif};
pub use image::{ImageInfo, image_info, image_info_from};

/// The most an index read (`moov`, Matroska headers) takes from a file.
pub const INDEX_LIMIT: u64 = 16 * 1024 * 1024;

/// `3:07`, `1:02:09`: a length as a player shows it.
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_read_like_a_player() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(187.4), "3:07");
        assert_eq!(clock(3729.0), "1:02:09");
    }
}
