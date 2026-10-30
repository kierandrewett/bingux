//! What the Compress dialog remembers between uses: the format, the level
//! and the options, in a small text file under `$XDG_CONFIG_HOME`
//! (`bingux/files/compress.conf`).
//!
//! The name, a password, "Delete originals" and "Replace original" are never
//! remembered: each is a choice about this one job, and the last two lose
//! data. The file is `key=value` lines; a line that is not understood is
//! ignored, so a newer file read by an older program still loads. It is
//! written to a part file and renamed, so a crash cannot leave half of it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::media_plan::{
    AudioCodec, ImageFormat, ReduceOptions, Sound, VideoCodec, VideoQuality,
};
use super::pack_plan::{Excludes, Format, Level, PackOptions};

/// The remembered choices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub pack: PackOptions,
    pub reduce: ReduceOptions,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            pack: PackOptions::default(),
            reduce: ReduceOptions::default(),
        }
    }
}

/// Where the file lives, or `None` with no home folder.
pub fn path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME").filter(|home| !home.is_empty())?).join(".config"),
    };
    Some(base.join("bingux/files/compress.conf"))
}

fn level_key(level: Level) -> &'static str {
    match level {
        Level::Fastest => "fastest",
        Level::Balanced => "balanced",
        Level::Smallest => "smallest",
    }
}

fn level_from(key: &str) -> Option<Level> {
    Level::ALL.into_iter().find(|level| level_key(*level) == key)
}

fn flag(value: &str) -> Option<bool> {
    match value {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

impl Prefs {
    /// Read the file; any trouble gives the defaults.
    pub fn load() -> Prefs {
        path().map_or_else(Prefs::default, |path| Prefs::load_from(&path))
    }

    pub fn load_from(path: &Path) -> Prefs {
        fs::read_to_string(path).map_or_else(|_| Prefs::default(), |text| Prefs::parse(&text))
    }

    /// Read `key=value` lines over the defaults.
    pub fn parse(text: &str) -> Prefs {
        let mut prefs = Prefs::default();
        let (pack, reduce) = (&mut prefs.pack, &mut prefs.reduce);
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "format" => {
                    if let Some(format) = Format::ALL.into_iter().find(|f| f.extension().trim_start_matches('.') == value) {
                        pack.format = format;
                    }
                }
                "level" => {
                    if let Some(level) = level_from(value) {
                        pack.level = level;
                    }
                }
                "hidden" => pack.include_hidden = flag(value).unwrap_or(pack.include_hidden),
                "follow_links" => pack.follow_links = flag(value).unwrap_or(pack.follow_links),
                "keep_owner" => pack.keep_owner = flag(value).unwrap_or(pack.keep_owner),
                "excludes" => {
                    if let Ok(excludes) = Excludes::parse(value) {
                        pack.excludes = excludes;
                    }
                }
                "image_format" => {
                    if let Some(format) = ImageFormat::from_key(value) {
                        reduce.image.format = format;
                    }
                }
                "image_quality" => {
                    if let Ok(quality) = value.parse::<u32>() {
                        reduce.image.quality = quality.clamp(1, 100) as u8;
                    }
                }
                "image_max" => {
                    if let Ok(cap) = value.parse::<u32>() {
                        reduce.image.max_dimension = (cap > 0).then_some(cap.min(65_535));
                    }
                }
                "image_strip" => reduce.image.strip_metadata = flag(value).unwrap_or(false),
                "video_codec" => {
                    if let Some(codec) = VideoCodec::from_key(value) {
                        reduce.video.codec = codec;
                    }
                }
                "video_quality" => {
                    if let Some(level) = level_from(value) {
                        reduce.video.quality = VideoQuality::Level(level);
                    } else if let Some(crf) = value.strip_prefix("crf:").and_then(|n| n.parse::<u8>().ok()) {
                        reduce.video.quality = VideoQuality::Crf(crf);
                    }
                }
                "video_height" => {
                    if let Ok(cap) = value.parse::<u32>() {
                        reduce.video.max_height = (cap > 0).then_some(cap);
                    }
                }
                "video_fps" => {
                    if let Ok(cap) = value.parse::<u32>() {
                        reduce.video.max_fps = (cap > 0).then_some(cap);
                    }
                }
                "video_sound" => {
                    if let Ok(kbps) = value.parse::<u32>() {
                        reduce.video.sound = if kbps == 0 { Sound::Remove } else { Sound::Kbps(kbps.clamp(8, 512)) };
                    }
                }
                "video_strip" => reduce.video.strip_metadata = flag(value).unwrap_or(false),
                "audio_codec" => {
                    if let Some(codec) = AudioCodec::from_key(value) {
                        reduce.audio.codec = codec;
                    }
                }
                "audio_kbps" => {
                    if let Ok(kbps) = value.parse::<u32>() {
                        reduce.audio.kbps = kbps.clamp(8, 512);
                    }
                }
                "audio_strip" => reduce.audio.strip_metadata = flag(value).unwrap_or(false),
                _ => {}
            }
        }
        prefs
    }

    /// The file's text.
    pub fn render(&self) -> String {
        let (pack, reduce) = (&self.pack, &self.reduce);
        let on = |value: bool| if value { "1" } else { "0" };
        let quality = match reduce.video.quality {
            VideoQuality::Level(level) => level_key(level).to_string(),
            VideoQuality::Crf(crf) => format!("crf:{crf}"),
        };
        let sound = match reduce.video.sound {
            Sound::Remove => 0,
            Sound::Kbps(kbps) => kbps,
        };
        format!(
            "format={}\nlevel={}\nhidden={}\nfollow_links={}\nkeep_owner={}\nexcludes={}\n\
             image_format={}\nimage_quality={}\nimage_max={}\nimage_strip={}\n\
             video_codec={}\nvideo_quality={}\nvideo_height={}\nvideo_fps={}\nvideo_sound={}\nvideo_strip={}\n\
             audio_codec={}\naudio_kbps={}\naudio_strip={}\n",
            pack.format.extension().trim_start_matches('.'),
            level_key(pack.level),
            on(pack.include_hidden),
            on(pack.follow_links),
            on(pack.keep_owner),
            pack.excludes.text(),
            reduce.image.format.key(),
            reduce.image.quality,
            reduce.image.max_dimension.unwrap_or(0),
            on(reduce.image.strip_metadata),
            reduce.video.codec.key(),
            quality,
            reduce.video.max_height.unwrap_or(0),
            reduce.video.max_fps.unwrap_or(0),
            sound,
            on(reduce.video.strip_metadata),
            reduce.audio.codec.key(),
            reduce.audio.kbps,
            on(reduce.audio.strip_metadata),
        )
    }

    /// Write the file, if there is a place for it.
    pub fn save(&self) -> io::Result<()> {
        match path() {
            Some(path) => self.save_to(&path),
            None => Ok(()),
        }
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let part = path.with_extension("conf.part");
        fs::write(&part, self.render())?;
        fs::rename(&part, path)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::Temp;
    use super::*;

    #[test]
    fn the_file_round_trips() {
        let mut prefs = Prefs::default();
        prefs.pack.format = Format::TarZst;
        prefs.pack.level = Level::Smallest;
        prefs.pack.include_hidden = false;
        prefs.pack.follow_links = true;
        prefs.pack.keep_owner = false;
        prefs.pack.excludes = Excludes::parse("*.tmp, node_modules").unwrap();
        prefs.reduce.image.format = ImageFormat::Webp;
        prefs.reduce.image.quality = 40;
        prefs.reduce.image.max_dimension = Some(1920);
        prefs.reduce.image.strip_metadata = true;
        prefs.reduce.video.codec = VideoCodec::Vp9;
        prefs.reduce.video.quality = VideoQuality::Crf(30);
        prefs.reduce.video.max_height = Some(720);
        prefs.reduce.video.max_fps = Some(30);
        prefs.reduce.video.sound = Sound::Remove;
        prefs.reduce.audio.codec = AudioCodec::Flac;
        prefs.reduce.audio.kbps = 192;
        assert_eq!(Prefs::parse(&prefs.render()), prefs);
        let temp = Temp::new("prefs");
        let file = temp.path().join("deep/dir/compress.conf");
        prefs.save_to(&file).unwrap();
        assert_eq!(Prefs::load_from(&file), prefs);
        assert!(!file.with_extension("conf.part").exists());
    }

    #[test]
    fn nothing_dangerous_or_personal_is_remembered() {
        let mut prefs = Prefs::default();
        prefs.pack.delete_originals = true;
        prefs.pack.password = Some(super::super::pack_plan::Password::new("hunter2"));
        prefs.reduce.replace_original = true;
        let text = prefs.render();
        assert!(!text.contains("hunter2") && !text.contains("delete") && !text.contains("replace"));
        let back = Prefs::parse(&text);
        assert!(!back.pack.delete_originals && back.pack.password.is_none() && !back.reduce.replace_original);
    }

    #[test]
    fn what_is_not_understood_is_ignored() {
        let prefs = Prefs::parse("format=rar\nlevel=\nfuture_key=1\nno equals sign\nimage_quality=900\nvideo_sound=9999\nexcludes=a[b\n");
        assert_eq!(prefs.pack.format, Format::Zip);
        assert_eq!(prefs.reduce.image.quality, 100);
        assert_eq!(prefs.reduce.video.sound, Sound::Kbps(512));
        assert!(prefs.pack.excludes.is_empty(), "a broken pattern list is dropped");
        assert_eq!(Prefs::parse(""), Prefs::default());
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let temp = Temp::new("prefs-missing");
        assert_eq!(Prefs::load_from(&temp.path().join("none.conf")), Prefs::default());
    }
}
