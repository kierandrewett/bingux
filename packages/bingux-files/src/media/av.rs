//! A recording's length and codecs, from the container's header.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::INDEX_LIMIT;

/// What the header of an audio or video file says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvInfo {
    pub seconds: Option<f64>,
    /// "H.264", "AAC": the codecs of the streams, video first, each once.
    pub codecs: Vec<String>,
}

impl AvInfo {
    /// "H.264, AAC", or `None` when no stream named its codec.
    pub fn codec_line(&self) -> Option<String> {
        (!self.codecs.is_empty()).then(|| self.codecs.join(", "))
    }

    fn codec(&mut self, name: String) {
        if !self.codecs.contains(&name) {
            self.codecs.push(name);
        }
    }
}

/// The length and codecs of the recording at `path`. `None` for a container
/// this does not read, or a file that is not what its name says.
pub fn av_info(path: &Path) -> Option<AvInfo> {
    av_info_from(File::open(path).ok()?)
}

/// [`av_info`] over any reader.
pub fn av_info_from(mut source: impl Read + Seek) -> Option<AvInfo> {
    let mut head = [0u8; 12];
    source.read_exact(&mut head).ok()?;
    source.seek(SeekFrom::Start(0)).ok()?;
    let info = if head.starts_with(b"RIFF") && &head[8..12] == b"WAVE" {
        wav(&mut source)?
    } else if head.starts_with(b"fLaC") {
        flac(&mut source)?
    } else if head.starts_with(b"OggS") {
        ogg(&mut source)?
    } else if matches!(&head[4..8], b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free") {
        mp4(&mut source)?
    } else if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        matroska(&mut source)?
    } else {
        return None;
    };
    Some(info)
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

// ---- WAV ---------------------------------------------------------------------

fn wav(source: &mut (impl Read + Seek)) -> Option<AvInfo> {
    let length = source.seek(SeekFrom::End(0)).ok()?;
    source.seek(SeekFrom::Start(12)).ok()?;
    let (mut byte_rate, mut codec) = (None, None);
    for _ in 0..32 {
        let mut chunk = [0u8; 8];
        source.read_exact(&mut chunk).ok()?;
        let size = u64::from(le32(&chunk, 4)?);
        let start = source.stream_position().ok()?;
        match &chunk[..4] {
            b"fmt " => {
                let mut format = [0u8; 16];
                source.read_exact(&mut format).ok()?;
                byte_rate = le32(&format, 8).filter(|rate| *rate > 0);
                codec = Some(
                    match le16(&format, 0)? {
                        1 | 0xFFFE => "PCM",
                        3 => "PCM float",
                        6 => "A-law",
                        7 => "µ-law",
                        0x55 => "MP3",
                        _ => "WAV",
                    }
                    .to_owned(),
                );
            }
            b"data" => {
                // Streamed files leave the size empty or at its maximum.
                let size = if size == 0 || size == 0xFFFF_FFFF {
                    length.saturating_sub(start)
                } else {
                    size.min(length.saturating_sub(start))
                };
                let mut info = AvInfo {
                    seconds: byte_rate.map(|rate| size as f64 / f64::from(rate)),
                    codecs: Vec::new(),
                };
                if let Some(codec) = codec {
                    info.codec(codec);
                }
                return Some(info);
            }
            _ => {}
        }
        // Chunks are padded to an even size.
        source.seek(SeekFrom::Start(start + size + (size & 1))).ok()?;
    }
    None
}

// ---- FLAC --------------------------------------------------------------------

fn flac(source: &mut (impl Read + Seek)) -> Option<AvInfo> {
    let mut block = [0u8; 4 + 4 + 18];
    source.read_exact(&mut block).ok()?;
    // The first metadata block must be STREAMINFO, type 0.
    if block[4] & 0x7F != 0 {
        return None;
    }
    let info = &block[8..];
    let rate = (u64::from(info[10]) << 12) | (u64::from(info[11]) << 4) | u64::from(info[12] >> 4);
    let total = (u64::from(info[13] & 0x0F) << 32)
        | (u64::from(info[14]) << 24)
        | (u64::from(info[15]) << 16)
        | (u64::from(info[16]) << 8)
        | u64::from(info[17]);
    let mut found = AvInfo {
        seconds: (rate > 0 && total > 0).then(|| total as f64 / rate as f64),
        codecs: Vec::new(),
    };
    found.codec("FLAC".to_owned());
    Some(found)
}

// ---- Ogg ---------------------------------------------------------------------

fn ogg(source: &mut (impl Read + Seek)) -> Option<AvInfo> {
    let length = source.seek(SeekFrom::End(0)).ok()?;
    source.seek(SeekFrom::Start(0)).ok()?;
    let mut page = [0u8; 27 + 255 + 64];
    let have = source.read(&mut page).ok()?;
    let page = &page[..have];
    let segments = usize::from(*page.get(26)?);
    let payload = page.get(27 + segments..)?;
    let (codec, rate, pre_skip) = if payload.starts_with(b"OpusHead") {
        ("Opus", 48_000.0, f64::from(le16(payload, 10)?))
    } else if payload.starts_with(b"\x01vorbis") {
        ("Vorbis", f64::from(le32(payload, 12)?), 0.0)
    } else {
        return None;
    };
    // The last page says how many samples the whole stream holds.
    let from = length.saturating_sub(65_536);
    source.seek(SeekFrom::Start(from)).ok()?;
    let mut tail = Vec::new();
    source.by_ref().take(65_536).read_to_end(&mut tail).ok()?;
    let granule = tail
        .windows(4)
        .rposition(|window| window == b"OggS")
        .and_then(|at| tail.get(at + 6..at + 14))
        .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
        .map(u64::from_le_bytes)
        .filter(|granule| *granule != u64::MAX);
    let mut found = AvInfo {
        seconds: granule
            .filter(|_| rate > 0.0)
            .map(|granule| ((granule as f64 - pre_skip) / rate).max(0.0)),
        codecs: Vec::new(),
    };
    found.codec(codec.to_owned());
    Some(found)
}

// ---- MP4, M4A, MOV -----------------------------------------------------------

/// The boxes of `data`: each one's type and payload.
fn boxes(data: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut at = 0;
    std::iter::from_fn(move || {
        let size = be32(data, at)? as usize;
        let kind = data.get(at + 4..at + 8)?;
        let (header, size) = match size {
            0 => (8, data.len() - at),
            1 => (16, be64(data, at + 8)? as usize),
            size => (8, size),
        };
        let payload = data.get(at + header..at.checked_add(size)?)?;
        at += size;
        Some((kind, payload))
    })
}

fn child<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).find(|(found, _)| found == kind).map(|(_, payload)| payload)
}

fn mp4(source: &mut (impl Read + Seek)) -> Option<AvInfo> {
    let length = source.seek(SeekFrom::End(0)).ok()?;
    let mut at = 0u64;
    // `moov` is at the front or the back; the media data between is skipped.
    for _ in 0..64 {
        if at + 8 > length {
            return None;
        }
        source.seek(SeekFrom::Start(at)).ok()?;
        let mut header = [0u8; 16];
        let have = source.read(&mut header).ok()?;
        let header = &header[..have];
        let size = match u64::from(be32(header, 0)?) {
            0 => length - at,
            1 => be64(header, 8)?,
            size => size,
        };
        if size < 8 {
            return None;
        }
        if header.get(4..8)? == b"moov" {
            let skip = if be32(header, 0)? == 1 { 16 } else { 8 };
            let payload = size.checked_sub(skip)?.min(length - at - skip);
            if payload > INDEX_LIMIT {
                return None;
            }
            source.seek(SeekFrom::Start(at + skip)).ok()?;
            let mut moov = vec![0u8; payload as usize];
            source.read_exact(&mut moov).ok()?;
            return Some(moov_info(&moov));
        }
        at += size;
    }
    None
}

fn moov_info(moov: &[u8]) -> AvInfo {
    let mut info = AvInfo::default();
    info.seconds = child(moov, b"mvhd").and_then(|mvhd| {
        let (scale, duration) = if *mvhd.first()? == 1 {
            (be32(mvhd, 20)?, be64(mvhd, 24)?)
        } else {
            (be32(mvhd, 12)?, u64::from(be32(mvhd, 16)?))
        };
        (scale > 0).then(|| duration as f64 / f64::from(scale))
    });
    let mut video = Vec::new();
    let mut audio = Vec::new();
    for (kind, track) in boxes(moov) {
        if kind != b"trak" {
            continue;
        }
        let Some(media) = child(track, b"mdia") else {
            continue;
        };
        let handler = child(media, b"hdlr").and_then(|hdlr| hdlr.get(8..12));
        // stsd: version and flags, a count, then the first entry's size and type.
        let name = child(media, b"minf")
            .and_then(|minf| child(minf, b"stbl"))
            .and_then(|stbl| child(stbl, b"stsd"))
            .and_then(|stsd| stsd.get(12..16))
            .map(codec_name);
        match (handler, name) {
            (Some(b"vide"), Some(name)) => video.push(name),
            (Some(b"soun"), Some(name)) => audio.push(name),
            _ => {}
        }
    }
    for name in video.into_iter().chain(audio) {
        info.codec(name);
    }
    info
}

fn codec_name(fourcc: &[u8]) -> String {
    match fourcc {
        b"avc1" | b"avc3" => "H.264",
        b"hvc1" | b"hev1" => "H.265",
        b"av01" => "AV1",
        b"vp09" => "VP9",
        b"vp08" => "VP8",
        b"mp4v" => "MPEG-4 video",
        b"mp4a" => "AAC",
        b"Opus" => "Opus",
        b"fLaC" => "FLAC",
        b"alac" => "ALAC",
        b"ac-3" => "AC-3",
        b"ec-3" => "E-AC-3",
        other => return String::from_utf8_lossy(other).trim().to_owned(),
    }
    .to_owned()
}

// ---- Matroska, WebM ------------------------------------------------------------

/// How much of the front of a Matroska file is read. Its Info and Tracks
/// elements come before the first cluster.
const MATROSKA_HEAD: u64 = 1024 * 1024;

/// One element header at `at`: its id, its payload size (`None` when
/// unknown) and where the payload starts.
fn element(data: &[u8], at: usize) -> Option<(u32, Option<u64>, usize)> {
    let first = *data.get(at)?;
    let id_length = first.leading_zeros() as usize + 1;
    if id_length > 4 {
        return None;
    }
    let mut id = 0u32;
    for byte in data.get(at..at + id_length)? {
        id = (id << 8) | u32::from(*byte);
    }
    let size_at = at + id_length;
    let lead = *data.get(size_at)?;
    let size_length = lead.leading_zeros() as usize + 1;
    if size_length > 8 {
        return None;
    }
    let mut size = u64::from(lead) & (0xFF >> size_length);
    let mut all_ones = size == (0xFF >> size_length);
    for byte in data.get(size_at + 1..size_at + size_length)? {
        size = (size << 8) | u64::from(*byte);
        all_ones &= *byte == 0xFF;
    }
    Some((id, (!all_ones).then_some(size), size_at + size_length))
}

fn matroska(source: &mut impl Read) -> Option<AvInfo> {
    let mut data = Vec::new();
    source.by_ref().take(MATROSKA_HEAD).read_to_end(&mut data).ok()?;
    let mut info = AvInfo::default();
    let (mut scale, mut duration) = (1_000_000u64, None);
    let mut codecs: Vec<(bool, String)> = Vec::new();
    // The elements that hold what is wanted are entered; others are skipped.
    const ENTER: [u32; 4] = [0x1853_8067, 0x1549_A966, 0x1654_AE6B, 0xAE];
    const CLUSTER: u32 = 0x1F43_B675;
    let mut at = 0;
    let mut is_video = false;
    while let Some((id, size, payload)) = element(&data, at) {
        if id == CLUSTER {
            break;
        }
        if ENTER.contains(&id) {
            at = payload;
            continue;
        }
        // An element of unknown size that is not a container ends the walk.
        let Some(end) = size.and_then(|size| payload.checked_add(usize::try_from(size).ok()?))
        else {
            break;
        };
        let Some(value) = data.get(payload..end) else {
            break;
        };
        match id {
            // TimestampScale
            0x2A_D7B1 => scale = value.iter().fold(0u64, |sum, byte| (sum << 8) | u64::from(*byte)),
            // Duration: a 4 or 8 byte float, in scale units
            0x4489 => {
                duration = match value {
                    [a, b, c, d] => Some(f64::from(f32::from_be_bytes([*a, *b, *c, *d]))),
                    bytes => <[u8; 8]>::try_from(bytes).ok().map(f64::from_be_bytes),
                };
            }
            // TrackType: 1 video, 2 audio
            0x83 => is_video = value.first() == Some(&1),
            // CodecID
            0x86 => {
                let id = String::from_utf8_lossy(value).into_owned();
                codecs.push((is_video, matroska_codec(&id)));
            }
            _ => {}
        }
        at = end;
    }
    info.seconds = duration.map(|ticks| ticks * scale as f64 / 1e9);
    codecs.sort_by_key(|(video, _)| !*video);
    for (_, name) in codecs {
        info.codec(name);
    }
    (info.seconds.is_some() || !info.codecs.is_empty()).then_some(info)
}

fn matroska_codec(id: &str) -> String {
    match id {
        "V_MPEG4/ISO/AVC" => "H.264",
        "V_MPEGH/ISO/HEVC" => "H.265",
        "V_VP8" => "VP8",
        "V_VP9" => "VP9",
        "V_AV1" => "AV1",
        "A_AAC" => "AAC",
        "A_OPUS" => "Opus",
        "A_VORBIS" => "Vorbis",
        "A_FLAC" => "FLAC",
        "A_AC3" => "AC-3",
        "A_EAC3" => "E-AC-3",
        "A_DTS" => "DTS",
        other => other,
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn read(bytes: Vec<u8>) -> Option<AvInfo> {
        av_info_from(Cursor::new(bytes))
    }

    #[test]
    fn a_wav_is_data_over_byte_rate() {
        let mut file = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes()); // PCM
        file.extend_from_slice(&2u16.to_le_bytes());
        file.extend_from_slice(&44_100u32.to_le_bytes());
        file.extend_from_slice(&176_400u32.to_le_bytes()); // byte rate
        file.extend_from_slice(&[4, 0, 16, 0]);
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(176_400u32 * 3).to_le_bytes());
        file.extend(std::iter::repeat_n(0u8, 176_400 * 3));
        let info = read(file).unwrap();
        assert_eq!(info.seconds, Some(3.0));
        assert_eq!(info.codec_line().as_deref(), Some("PCM"));
    }

    #[test]
    fn a_flac_streaminfo_gives_the_length() {
        let mut file = b"fLaC".to_vec();
        file.extend_from_slice(&[0x80, 0, 0, 34]);
        let mut info = [0u8; 34];
        // 44100 Hz = 0x0AC44: bytes 0x0A 0xC4 0x4_, 2 channels, 16 bits.
        info[10] = 0x0A;
        info[11] = 0xC4;
        info[12] = 0x40 | (1 << 1);
        info[13] = 0xF0; // low bits of bits-per-sample, and total samples' top
        // total samples 44100 * 5 = 220500 in the low 32 bits.
        info[14..18].copy_from_slice(&220_500u32.to_be_bytes());
        file.extend_from_slice(&info);
        let found = read(file).unwrap();
        assert_eq!(found.seconds, Some(5.0));
        assert_eq!(found.codec_line().as_deref(), Some("FLAC"));
    }

    fn ogg_page(granule: u64, payload: &[u8]) -> Vec<u8> {
        let mut page = b"OggS\0\x02".to_vec();
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&[0; 12]);
        page.push(1);
        page.push(payload.len() as u8);
        page.extend_from_slice(payload);
        page
    }

    #[test]
    fn an_opus_stream_counts_from_its_last_granule() {
        let mut head = b"OpusHead\x01\x02".to_vec();
        head.extend_from_slice(&312u16.to_le_bytes());
        head.extend_from_slice(&[0; 8]);
        let mut file = ogg_page(0, &head);
        file.extend(ogg_page(48_000 * 10 + 312, b"audio"));
        let info = read(file).unwrap();
        assert_eq!(info.seconds, Some(10.0));
        assert_eq!(info.codec_line().as_deref(), Some("Opus"));
    }

    #[test]
    fn a_vorbis_stream_uses_its_own_rate() {
        let mut head = b"\x01vorbis\0\0\0\0\x02".to_vec();
        head.extend_from_slice(&22_050u32.to_le_bytes());
        let mut file = ogg_page(0, &head);
        file.extend(ogg_page(22_050 * 4, b"audio"));
        assert_eq!(read(file).unwrap().seconds, Some(4.0));
    }

    fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut bytes = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(payload);
        bytes
    }

    fn track(handler: &[u8; 4], codec: &[u8; 4]) -> Vec<u8> {
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(handler);
        let mut stsd = vec![0u8; 4];
        stsd.extend_from_slice(&1u32.to_be_bytes());
        stsd.extend(bx(codec, &[0; 8]));
        let stbl = bx(b"stbl", &bx(b"stsd", &stsd));
        let minf = bx(b"minf", &stbl);
        let mut mdia = bx(b"hdlr", &hdlr);
        mdia.extend(minf);
        bx(b"trak", &bx(b"mdia", &mdia))
    }

    #[test]
    fn an_mp4_reads_moov_after_skipping_the_media() {
        let mut mvhd = vec![0u8; 4];
        mvhd.extend_from_slice(&[0; 8]);
        mvhd.extend_from_slice(&1000u32.to_be_bytes());
        mvhd.extend_from_slice(&90_500u32.to_be_bytes());
        mvhd.extend_from_slice(&[0; 80]);
        let mut moov = bx(b"mvhd", &mvhd);
        moov.extend(track(b"vide", b"avc1"));
        moov.extend(track(b"soun", b"mp4a"));
        let mut file = bx(b"ftyp", b"isom\0\0\0\0isom");
        file.extend(bx(b"mdat", &[7u8; 5000]));
        file.extend(bx(b"moov", &moov));
        let info = read(file).unwrap();
        assert_eq!(info.seconds, Some(90.5));
        assert_eq!(info.codec_line().as_deref(), Some("H.264, AAC"));
    }

    fn ebml(id: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut bytes = id.to_vec();
        bytes.push(0x80 | payload.len() as u8);
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn a_matroska_duration_is_in_timestamp_units() {
        let mut info = ebml(&[0x2A, 0xD7, 0xB1], &1_000_000u32.to_be_bytes());
        info.extend(ebml(&[0x44, 0x89], &125_500.0f32.to_be_bytes()));
        let mut video = ebml(&[0x83], &[1]);
        video.extend(ebml(&[0x86], b"V_VP9"));
        let mut audio = ebml(&[0x83], &[2]);
        audio.extend(ebml(&[0x86], b"A_OPUS"));
        let mut tracks = ebml(&[0xAE], &audio);
        tracks.extend(ebml(&[0xAE], &video));
        let mut segment = ebml(&[0x15, 0x49, 0xA9, 0x66], &info);
        segment.extend(ebml(&[0x16, 0x54, 0xAE, 0x6B], &tracks));
        let mut file = ebml(&[0x1A, 0x45, 0xDF, 0xA3], &ebml(&[0x42, 0x82], b"webm"));
        file.extend(ebml(&[0x18, 0x53, 0x80, 0x67], &segment));
        let found = read(file).unwrap();
        assert_eq!(found.seconds, Some(125.5));
        assert_eq!(found.codec_line().as_deref(), Some("VP9, Opus"));
    }

    #[test]
    fn unknown_and_cut_files_give_nothing() {
        assert_eq!(read(b"hello world, this is text".to_vec()), None);
        assert_eq!(read(b"RIFF\0\0\0\0WAVE".to_vec()), None);
        assert_eq!(read(Vec::new()), None);
    }
}
