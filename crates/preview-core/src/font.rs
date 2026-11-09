//! A font's names, metrics and character set, for the specimen viewer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bingux_files::Cancel;

use crate::registry::LoadError;
use crate::source::{PreviewSource, read_at};
use crate::text::plain;

/// The largest font file read.
pub const LIMIT: usize = 32 * 1024 * 1024;

/// The bytes of the font file. The viewer hands them to the text engine, so
/// they travel with the facts and the file is read once. Cloning shares them.
#[derive(Clone)]
pub struct FontData(Arc<[u8]>);

impl FontData {
    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.0
    }
}

impl std::fmt::Debug for FontData {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "FontData({} bytes)", self.0.len())
    }
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for FontData {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFacts {
    pub family: String,
    pub style: Option<String>,
    pub full_name: Option<String>,
    pub version: Option<String>,
    pub designer: Option<String>,
    pub glyphs: u16,
    /// 100 to 900.
    pub weight: u16,
    pub italic: bool,
    pub monospaced: bool,
    pub faces: u32,
    /// Every printable character the font draws, in code point order.
    pub characters: Vec<char>,
    pub data: FontData,
}

/// The most characters listed: a font of every CJK ideograph has about 90 000.
const CHARACTER_LIMIT: usize = 100_000;

/// Read the names of the font in `source`. Blocks on the disk.
pub fn read(source: &PreviewSource, cancel: &Cancel) -> Result<FontFacts, LoadError> {
    let mut reader = source.open().map_err(|error| LoadError::Unreadable(plain(&error)))?;
    let data = read_at(&mut *reader, 0, LIMIT).map_err(|error| LoadError::Unreadable(plain(&error)))?;
    if cancel.is_cancelled() {
        return Err(LoadError::Cancelled);
    }
    facts(data)
}

/// The printable characters the face has a glyph for.
fn characters(face: &ttf_parser::Face<'_>) -> Vec<char> {
    let mut found = BTreeSet::new();
    if let Some(cmap) = face.tables().cmap {
        for subtable in cmap.subtables.into_iter().filter(|subtable| subtable.is_unicode()) {
            subtable.codepoints(|codepoint| {
                if found.len() < CHARACTER_LIMIT
                    && let Some(character) = char::from_u32(codepoint)
                    && !character.is_control()
                    && !character.is_whitespace()
                    && subtable.glyph_index(codepoint).is_some_and(|glyph| glyph.0 != 0)
                {
                    found.insert(character);
                }
            });
        }
    }
    found.into_iter().collect()
}

fn facts(data: Vec<u8>) -> Result<FontFacts, LoadError> {
    use ttf_parser::name_id;
    let face = ttf_parser::Face::parse(&data, 0).map_err(|_| LoadError::Unsupported("This is not a font Bingux can read.".to_owned()))?;
    let name = |id: u16| {
        face.names()
            .into_iter()
            .filter(|name| name.name_id == id)
            .find_map(|name| name.to_string())
            .filter(|text| !text.trim().is_empty())
    };
    let family = name(name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| name(name_id::FAMILY))
        .ok_or_else(|| LoadError::Unsupported("This font has no name.".to_owned()))?;
    Ok(FontFacts {
        family,
        style: name(name_id::TYPOGRAPHIC_SUBFAMILY).or_else(|| name(name_id::SUBFAMILY)),
        full_name: name(name_id::FULL_NAME),
        version: name(name_id::VERSION),
        designer: name(name_id::DESIGNER),
        glyphs: face.number_of_glyphs(),
        weight: face.weight().to_number(),
        italic: face.is_italic(),
        monospaced: face.is_monospaced(),
        faces: ttf_parser::fonts_in_collection(&data).unwrap_or(1),
        characters: characters(&face),
        data: FontData(data.into()),
    })
}

/// Where a person's own fonts live: `$XDG_DATA_HOME/fonts`, or
/// `~/.local/share/fonts`. Fontconfig and every toolkit look there.
pub fn user_font_dir() -> Option<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(data_home.join("fonts"))
}

/// What [`install`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// The font was written here.
    New(PathBuf),
    /// This very file was already there.
    AlreadyThere(PathBuf),
}

/// Whether `dir` already holds a file called `file_name` with exactly `data`.
pub fn is_installed(data: &[u8], file_name: &str, dir: &Path) -> bool {
    let path = dir.join(file_name);
    std::fs::metadata(&path).is_ok_and(|meta| meta.len() == data.len() as u64)
        && std::fs::read(&path).is_ok_and(|existing| existing == data)
}

/// Put the font file in `dir` for the person, never replacing another file:
/// a different file with the same name gets the next free "name (copy).ttf".
/// `file_name` is only used as a name; any folders in it are dropped.
pub fn install(data: &[u8], file_name: &str, dir: &Path) -> std::io::Result<Installed> {
    let name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && !name.starts_with('.'))
        .unwrap_or("font.ttf");
    if is_installed(data, name, dir) {
        return Ok(Installed::AlreadyThere(dir.join(name)));
    }
    std::fs::create_dir_all(dir)?;
    let name = if dir.join(name).exists() {
        bingux_files::ops::keep_both_name(name, |candidate| dir.join(candidate).exists())
    } else {
        name.to_owned()
    };
    let target = dir.join(name);
    // Written beside the target and renamed, so a half-written font is never on the path.
    let partial = target.with_extension("part");
    std::fs::write(&partial, data)?;
    std::fs::rename(&partial, &target)?;
    Ok(Installed::New(target))
}

/// Ask fontconfig to look at `dir` again, so programs that are already
/// running find the font. Best effort: without `fc-cache` the font is still
/// installed and appears the next time fontconfig scans.
pub fn refresh_cache(dir: &Path) {
    let _ = std::process::Command::new("fc-cache")
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_font_reads_its_names() {
        let Some(path) = [
            "/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf",
            "/usr/share/fonts/adwaita/AdwaitaSans-Regular.ttf",
            "/var/tmp/PreviewKit-home/media/AdwaitaSans-Regular.ttf",
        ]
            .into_iter()
            .find(|path| std::path::Path::new(path).exists())
        else {
            return;
        };
        let facts = read(&PreviewSource::path(path), &Cancel::new()).unwrap();
        assert!(facts.family.contains("Adwaita"), "{facts:?}");
        assert!(facts.glyphs > 100);
        assert!(facts.characters.contains(&'A') && facts.characters.contains(&'é'), "{} characters", facts.characters.len());
        assert!(facts.characters.windows(2).all(|pair| pair[0] < pair[1]), "in code point order, without repeats");
        assert!(!facts.characters.iter().any(|character| character.is_whitespace() || character.is_control()));
    }

    #[test]
    fn not_a_font_is_unsupported() {
        assert!(matches!(read(&PreviewSource::bytes("a.ttf", vec![0, 1, 0, 0, 9, 9]), &Cancel::new()), Err(LoadError::Unsupported(_))));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bingux-font-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn installing_writes_once_and_never_replaces() {
        let dir = scratch("install");
        let first = install(b"one", "Face.ttf", &dir).unwrap();
        assert_eq!(first, Installed::New(dir.join("Face.ttf")));
        assert!(is_installed(b"one", "Face.ttf", &dir));
        assert_eq!(install(b"one", "Face.ttf", &dir).unwrap(), Installed::AlreadyThere(dir.join("Face.ttf")));
        // Another file of the same name is kept beside the first.
        let Installed::New(other) = install(b"two", "Face.ttf", &dir).unwrap() else {
            panic!("a different file is new");
        };
        assert_ne!(other, dir.join("Face.ttf"));
        assert_eq!(std::fs::read(dir.join("Face.ttf")).unwrap(), b"one");
        assert_eq!(std::fs::read(&other).unwrap(), b"two");
        assert!(!dir.join("Face.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_install_name_has_no_folders_and_is_not_hidden() {
        let dir = scratch("name");
        let Installed::New(path) = install(b"x", "../../etc/Evil.otf", &dir).unwrap() else {
            panic!("new");
        };
        assert_eq!(path, dir.join("Evil.otf"));
        let Installed::New(path) = install(b"x", ".hidden", &dir).unwrap() else {
            panic!("new");
        };
        assert_eq!(path, dir.join("font.ttf"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
