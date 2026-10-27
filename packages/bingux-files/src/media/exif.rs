//! The part of EXIF a Properties dialog shows: camera, lens, when and how a
//! photo was taken, and its orientation.

/// What a photo's EXIF says. Every field is empty when the tag is absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exif {
    /// "Canon EOS R5": the maker and model, without saying the maker twice.
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// "14 Mar 2024, 14:03": when the shutter was pressed.
    pub taken: Option<String>,
    /// "1/250 s, f/2.8, ISO 200, 35 mm".
    pub exposure: Option<String>,
    /// 1 to 8; 5 to 8 mean the picture is stored on its side.
    pub orientation: Option<u16>,
}

impl Exif {
    pub fn is_empty(&self) -> bool {
        *self == Exif::default()
    }

    /// The picture is stored turned a quarter, so width and height swap.
    pub fn is_sideways(&self) -> bool {
        self.orientation.is_some_and(|value| (5..=8).contains(&value))
    }
}

const MAKE: u16 = 0x010F;
const MODEL: u16 = 0x0110;
const ORIENTATION: u16 = 0x0112;
const EXIF_IFD: u16 = 0x8769;
const EXPOSURE_TIME: u16 = 0x829A;
const F_NUMBER: u16 = 0x829D;
const ISO: u16 = 0x8827;
const DATE_ORIGINAL: u16 = 0x9003;
const FOCAL_LENGTH: u16 = 0x920A;
const LENS_MODEL: u16 = 0xA434;

const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;

/// A TIFF block: the byte order and the bytes every offset points into.
struct Tiff<'a> {
    data: &'a [u8],
    little: bool,
}

/// One IFD entry.
struct Field {
    tag: u16,
    kind: u16,
    count: u32,
    /// Where the value's bytes are: inline in the entry or at an offset.
    at: usize,
}

impl<'a> Tiff<'a> {
    fn u16(&self, at: usize) -> Option<u16> {
        let bytes: [u8; 2] = self.data.get(at..at + 2)?.try_into().ok()?;
        Some(if self.little {
            u16::from_le_bytes(bytes)
        } else {
            u16::from_be_bytes(bytes)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let bytes: [u8; 4] = self.data.get(at..at + 4)?.try_into().ok()?;
        Some(if self.little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    /// The entries of the IFD at `offset`. At most 256, so a corrupt count
    /// cannot make a long loop.
    fn fields(&self, offset: usize) -> Vec<Field> {
        let Some(count) = self.u16(offset) else {
            return Vec::new();
        };
        (0..usize::from(count).min(256))
            .filter_map(|index| {
                let entry = offset + 2 + index * 12;
                let kind = self.u16(entry + 2)?;
                let count = self.u32(entry + 4)?;
                let size = match kind {
                    ASCII | 1 | 7 => 1,
                    SHORT => 2,
                    LONG => 4,
                    RATIONAL => 8,
                    _ => return None,
                } * count as usize;
                let at = if size <= 4 {
                    entry + 8
                } else {
                    self.u32(entry + 8)? as usize
                };
                Some(Field {
                    tag: self.u16(entry)?,
                    kind,
                    count,
                    at,
                })
            })
            .collect()
    }

    fn text(&self, field: &Field) -> Option<String> {
        if field.kind != ASCII {
            return None;
        }
        let bytes = self.data.get(field.at..field.at + field.count as usize)?;
        let text = String::from_utf8_lossy(bytes);
        let text = text.trim_end_matches('\0').trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    fn number(&self, field: &Field) -> Option<u32> {
        match field.kind {
            SHORT => self.u16(field.at).map(u32::from),
            LONG => self.u32(field.at),
            _ => None,
        }
    }

    fn rational(&self, field: &Field) -> Option<(u32, u32)> {
        (field.kind == RATIONAL)
            .then(|| Some((self.u32(field.at)?, self.u32(field.at + 4)?)))
            .flatten()
    }
}

/// Read the EXIF of a TIFF block (what follows `Exif\0\0` in a JPEG's APP1
/// segment). `None` when it is not a TIFF block.
pub fn parse_exif(data: &[u8]) -> Option<Exif> {
    let little = match data.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let tiff = Tiff { data, little };
    if tiff.u16(2)? != 42 {
        return None;
    }
    let mut exif = Exif::default();
    let (mut make, mut model, mut exposure_ifd) = (None, None, None);
    for field in tiff.fields(tiff.u32(4)? as usize) {
        match field.tag {
            MAKE => make = tiff.text(&field),
            MODEL => model = tiff.text(&field),
            ORIENTATION => exif.orientation = tiff.number(&field).map(|value| value as u16),
            EXIF_IFD => exposure_ifd = tiff.number(&field),
            _ => {}
        }
    }
    exif.camera = camera_name(make, model);
    let (mut time, mut f_number, mut iso, mut focal) = (None, None, None, None);
    if let Some(offset) = exposure_ifd {
        for field in tiff.fields(offset as usize) {
            match field.tag {
                DATE_ORIGINAL => exif.taken = tiff.text(&field).and_then(|text| exif_date(&text)),
                LENS_MODEL => exif.lens = tiff.text(&field),
                EXPOSURE_TIME => time = tiff.rational(&field),
                F_NUMBER => f_number = tiff.rational(&field),
                ISO => iso = tiff.number(&field),
                FOCAL_LENGTH => focal = tiff.rational(&field),
                _ => {}
            }
        }
    }
    exif.exposure = exposure_line(time, f_number, iso, focal);
    Some(exif)
}

/// "Canon EOS R5" from maker "Canon" and model "Canon EOS R5", or
/// "Apple iPhone 15" from "Apple" and "iPhone 15".
fn camera_name(make: Option<String>, model: Option<String>) -> Option<String> {
    match (make, model) {
        (Some(make), Some(model)) => {
            if model.to_lowercase().starts_with(&make.to_lowercase()) {
                Some(model)
            } else {
                Some(format!("{make} {model}"))
            }
        }
        (one, other) => one.or(other),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// "14 Mar 2024, 14:03" from EXIF's "2024:03:14 14:03:55".
pub(super) fn exif_date(text: &str) -> Option<String> {
    let (date, time) = text.split_once(' ')?;
    let mut date = date.split(':');
    let (year, month, day): (u32, usize, u32) = (
        date.next()?.parse().ok()?,
        date.next()?.parse().ok()?,
        date.next()?.parse().ok()?,
    );
    let mut time = time.split(':');
    let (hour, minute): (u32, u32) = (time.next()?.parse().ok()?, time.next()?.parse().ok()?);
    if year == 0 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!(
        "{day} {} {year}, {hour:02}:{minute:02}",
        MONTHS[month - 1]
    ))
}

/// "1/250 s, f/2.8, ISO 200, 35 mm".
fn exposure_line(
    time: Option<(u32, u32)>,
    f_number: Option<(u32, u32)>,
    iso: Option<u32>,
    focal: Option<(u32, u32)>,
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some((top, bottom)) = time.filter(|(_, bottom)| *bottom != 0) {
        parts.push(if top == 0 {
            "0 s".to_owned()
        } else if top < bottom {
            format!("1/{} s", (f64::from(bottom) / f64::from(top)).round())
        } else {
            format!("{} s", trim(f64::from(top) / f64::from(bottom)))
        });
    }
    if let Some((top, bottom)) = f_number.filter(|(_, bottom)| *bottom != 0) {
        parts.push(format!("f/{}", trim(f64::from(top) / f64::from(bottom))));
    }
    if let Some(iso) = iso {
        parts.push(format!("ISO {iso}"));
    }
    if let Some((top, bottom)) = focal.filter(|(_, bottom)| *bottom != 0) {
        parts.push(format!("{} mm", trim(f64::from(top) / f64::from(bottom))));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// One decimal place, none when it would be ".0".
fn trim(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

#[cfg(test)]
pub(in crate::media) mod tests {
    use super::*;

    /// A little-endian TIFF with Make, Model, Orientation and an Exif IFD
    /// holding the date, 1/250 s, f/2.8, ISO 200 and 35 mm.
    pub(in crate::media) fn sample() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"II*\0");
        data.extend_from_slice(&8u32.to_le_bytes());
        // IFD0 at 8: four entries, 2 + 48 + 4 bytes, then the strings.
        let strings_at = 8 + 2 + 4 * 12 + 4;
        let make = b"Canon\0";
        let model = b"Canon EOS R5\0";
        let exif_at = strings_at + make.len() + model.len();
        let entry = |tag: u16, kind: u16, count: u32, value: u32| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
            bytes
        };
        data.extend_from_slice(&4u16.to_le_bytes());
        data.extend(entry(MAKE, ASCII, make.len() as u32, strings_at as u32));
        data.extend(entry(
            MODEL,
            ASCII,
            model.len() as u32,
            (strings_at + make.len()) as u32,
        ));
        data.extend(entry(ORIENTATION, SHORT, 1, 6));
        data.extend(entry(EXIF_IFD, LONG, 1, exif_at as u32));
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(make);
        data.extend_from_slice(model);
        assert_eq!(data.len(), exif_at);
        // The Exif IFD: four entries; values after it.
        let values_at = exif_at + 2 + 4 * 12 + 4;
        let date = b"2024:03:14 14:03:55\0";
        data.extend_from_slice(&4u16.to_le_bytes());
        data.extend(entry(DATE_ORIGINAL, ASCII, date.len() as u32, values_at as u32));
        data.extend(entry(EXPOSURE_TIME, RATIONAL, 1, (values_at + 20) as u32));
        data.extend(entry(F_NUMBER, RATIONAL, 1, (values_at + 28) as u32));
        data.extend(entry(ISO, SHORT, 1, 200));
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(date);
        for (top, bottom) in [(1u32, 250u32), (28, 10)] {
            data.extend_from_slice(&top.to_le_bytes());
            data.extend_from_slice(&bottom.to_le_bytes());
        }
        data
    }

    #[test]
    fn camera_date_and_exposure_are_read() {
        let exif = parse_exif(&sample()).expect("exif");
        assert_eq!(exif.camera.as_deref(), Some("Canon EOS R5"));
        assert_eq!(exif.taken.as_deref(), Some("14 Mar 2024, 14:03"));
        assert_eq!(exif.exposure.as_deref(), Some("1/250 s, f/2.8, ISO 200"));
        assert_eq!(exif.orientation, Some(6));
        assert!(exif.is_sideways());
    }

    #[test]
    fn garbage_is_not_exif() {
        assert_eq!(parse_exif(b"nope"), None);
        assert_eq!(parse_exif(b"II*\0\xff\xff\xff\xff"), Some(Exif::default()));
        assert_eq!(parse_exif(b"MM\0*"), None);
    }

    #[test]
    fn the_maker_is_not_said_twice() {
        assert_eq!(
            camera_name(Some("Apple".into()), Some("iPhone 15".into())).as_deref(),
            Some("Apple iPhone 15")
        );
        assert_eq!(
            camera_name(Some("SONY".into()), Some("Sony ILCE-7".into())).as_deref(),
            Some("Sony ILCE-7")
        );
    }

    #[test]
    fn dates_and_exposures_read_like_a_camera_app() {
        assert_eq!(exif_date("2024:12:01 09:05:00").as_deref(), Some("1 Dec 2024, 09:05"));
        assert_eq!(exif_date("0000:00:00 00:00:00"), None);
        assert_eq!(
            exposure_line(Some((2, 1)), Some((18, 10)), None, Some((850, 10))).as_deref(),
            Some("2 s, f/1.8, 85 mm")
        );
    }
}
