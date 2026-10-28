//! A font's names and metrics, for the specimen viewer.

use bingux_files::Cancel;

use crate::registry::LoadError;
use crate::source::{PreviewSource, read_at};
use crate::text::plain;

/// The largest font file read.
pub const LIMIT: usize = 32 * 1024 * 1024;

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
}

/// Read the names of the font in `source`. Blocks on the disk.
pub fn read(source: &PreviewSource, cancel: &Cancel) -> Result<FontFacts, LoadError> {
    let mut reader = source.open().map_err(|error| LoadError::Unreadable(plain(&error)))?;
    let data = read_at(&mut *reader, 0, LIMIT).map_err(|error| LoadError::Unreadable(plain(&error)))?;
    if cancel.is_cancelled() {
        return Err(LoadError::Cancelled);
    }
    facts(&data)
}

fn facts(data: &[u8]) -> Result<FontFacts, LoadError> {
    use ttf_parser::name_id;
    let face = ttf_parser::Face::parse(data, 0).map_err(|_| LoadError::Unsupported("This is not a font Bingux can read.".to_owned()))?;
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
        faces: ttf_parser::fonts_in_collection(data).unwrap_or(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_font_reads_its_names() {
        let Some(path) = ["/usr/share/fonts/adwaita-sans-fonts/AdwaitaSans-Regular.ttf", "/var/tmp/PreviewKit-home/media/AdwaitaSans-Regular.ttf"]
            .into_iter()
            .find(|path| std::path::Path::new(path).exists())
        else {
            return;
        };
        let facts = read(&PreviewSource::path(path), &Cancel::new()).unwrap();
        assert!(facts.family.contains("Adwaita"), "{facts:?}");
        assert!(facts.glyphs > 100);
    }

    #[test]
    fn not_a_font_is_unsupported() {
        assert!(matches!(read(&PreviewSource::bytes("a.ttf", vec![0, 1, 0, 0, 9, 9]), &Cancel::new()), Err(LoadError::Unsupported(_))));
    }
}
