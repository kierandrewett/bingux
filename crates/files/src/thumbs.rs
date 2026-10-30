//! Finding thumbnails that are already made, by the freedesktop.org
//! Thumbnail Managing Standard.
//!
//! This module only **looks things up**. It decodes nothing and makes
//! nothing: a file manager thumbnailer (or the GNOME one that fills
//! `~/.cache/thumbnails`) does that. A lookup costs one `open` per cache
//! folder it tries and never touches the file it is looking for, so it is
//! cheap for every visible row, even on a network mount: the caller brings
//! the modified time it already has.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::location::file_uri;

/// A thumbnail size class from the standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bucket {
    /// 128 px.
    Normal,
    /// 256 px.
    Large,
    /// 512 px.
    XLarge,
    /// 1024 px.
    XXLarge,
}

impl Bucket {
    /// The smallest class that is at least `px` wide, or the biggest.
    pub fn for_size(px: f32) -> Bucket {
        if px <= 128.0 {
            Bucket::Normal
        } else if px <= 256.0 {
            Bucket::Large
        } else if px <= 512.0 {
            Bucket::XLarge
        } else {
            Bucket::XXLarge
        }
    }

    /// The longest side of the thumbnails in this class.
    pub fn px(self) -> u32 {
        match self {
            Bucket::Normal => 128,
            Bucket::Large => 256,
            Bucket::XLarge => 512,
            Bucket::XXLarge => 1024,
        }
    }

    /// The cache folder's name.
    fn folder(self) -> &'static str {
        match self {
            Bucket::Normal => "normal",
            Bucket::Large => "large",
            Bucket::XLarge => "x-large",
            Bucket::XXLarge => "xx-large",
        }
    }

    /// This class and every bigger one, smallest first.
    fn and_larger(self) -> impl Iterator<Item = Bucket> {
        [
            Bucket::Normal,
            Bucket::Large,
            Bucket::XLarge,
            Bucket::XXLarge,
        ]
        .into_iter()
        .filter(move |bucket| *bucket >= self)
    }
}

// ---- md5 --------------------------------------------------------------------

const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// The MD5 digest (RFC 1321). The standard names thumbnails by it; it is
/// not used for security.
fn md5(data: &[u8]) -> [u8; 16] {
    // K[i] = floor(2^32 * abs(sin(i + 1))).
    let constants: [u32; 64] =
        std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32);
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(data.len() as u64).wrapping_mul(8).to_le_bytes());

    for block in message.chunks_exact(64) {
        let words: [u32; 16] = std::array::from_fn(|i| {
            u32::from_le_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ])
        });
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (mix, index) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let sum = a
                .wrapping_add(mix)
                .wrapping_add(constants[i])
                .wrapping_add(words[index]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(SHIFTS[i]));
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    for (chunk, word) in digest.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    digest
}

/// The file name a thumbnail of `uri` has: the lowercase hex MD5 of the
/// `file://` URI, then `.png`.
pub fn thumbnail_name(uri: &str) -> String {
    let mut name = String::with_capacity(36);
    for byte in md5(uri.as_bytes()) {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".png");
    name
}

// ---- PNG --------------------------------------------------------------------

/// How much of a thumbnail is read to find its modified time. The `tEXt`
/// chunks sit before the image data, but `Thumb::URI` can be long.
const HEAD: usize = 4096;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// The `Thumb::MTime` of a PNG, from its first bytes. Looks at whole
/// chunks only and stops at the image data: nothing is decoded and the
/// checksums are not checked.
fn png_mtime(head: &[u8]) -> Option<u64> {
    let mut rest = head.strip_prefix(&PNG_SIGNATURE)?;
    while rest.len() >= 12 {
        let length = u32::from_be_bytes(rest[..4].try_into().ok()?) as usize;
        let kind = &rest[4..8];
        if kind == b"IDAT" || kind == b"IEND" {
            return None;
        }
        let data = rest.get(8..8usize.checked_add(length)?)?;
        if kind == b"tEXt"
            && let Some(split) = data.iter().position(|byte| *byte == 0)
            && &data[..split] == b"Thumb::MTime"
        {
            return std::str::from_utf8(&data[split + 1..])
                .ok()?
                .trim()
                .parse()
                .ok();
        }
        // Length, type, data, checksum.
        rest = rest.get(12 + length..)?;
    }
    None
}

/// Read the head of a thumbnail: `None` if it cannot be opened.
fn read_head(path: &Path) -> Option<Vec<u8>> {
    let mut head = Vec::with_capacity(HEAD);
    File::open(path)
        .ok()?
        .take(HEAD as u64)
        .read_to_end(&mut head)
        .ok()?;
    Some(head)
}

/// Is the thumbnail at `path` for a source modified at `modified`? With no
/// time to compare, any thumbnail that opens will do.
fn matches(path: &Path, modified: Option<SystemTime>) -> bool {
    let Some(head) = read_head(path) else {
        return false;
    };
    let Some(modified) = modified else {
        return true;
    };
    let Ok(since_epoch) = modified.duration_since(UNIX_EPOCH) else {
        return false;
    };
    png_mtime(&head) == Some(since_epoch.as_secs())
}

// ---- the cache ----------------------------------------------------------------

/// `$XDG_CACHE_HOME/thumbnails`, else `~/.cache/thumbnails`.
fn cache_root() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(dir) if Path::new(&dir).is_absolute() => PathBuf::from(dir),
        _ => {
            PathBuf::from(std::env::var_os("HOME").filter(|home| !home.is_empty())?).join(".cache")
        }
    };
    Some(base.join("thumbnails"))
}

/// Find a thumbnail for `path` that is not older than the file.
///
/// Looks in the `bucket` folder of the cache, then in each bigger one, and
/// returns the first thumbnail whose recorded `Thumb::MTime` equals
/// `modified` (whole seconds). With `modified` as `None` any thumbnail is
/// accepted. The source file is never touched.
pub fn lookup(path: &Path, modified: Option<SystemTime>, bucket: Bucket) -> Option<PathBuf> {
    lookup_in(&cache_root()?, path, modified, bucket)
}

fn lookup_in(
    root: &Path,
    path: &Path,
    modified: Option<SystemTime>,
    bucket: Bucket,
) -> Option<PathBuf> {
    let name = thumbnail_name(&file_uri(path));
    bucket
        .and_larger()
        .map(|bucket| root.join(bucket.folder()).join(&name))
        .find(|candidate| matches(candidate, modified))
}

/// Has a thumbnailer already failed on this file, at this modified time?
/// Reads the `fail` folders of the cache; the source file is not touched.
pub fn is_failed(path: &Path, modified: Option<SystemTime>) -> bool {
    cache_root().is_some_and(|root| is_failed_in(&root, path, modified))
}

fn is_failed_in(root: &Path, path: &Path, modified: Option<SystemTime>) -> bool {
    let name = thumbnail_name(&file_uri(path));
    let Ok(programs) = std::fs::read_dir(root.join("fail")) else {
        return false;
    };
    programs
        .flatten()
        .any(|program| matches(&program.path().join(&name), modified))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn hex(bytes: [u8; 16]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn md5_matches_known_digests() {
        // RFC 1321 test suite, and `md5sum` for the rest.
        let vectors = [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
            (
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
                "76658de2ac7d406f93dfbe8bb6d9f549",
            ),
        ];
        for (text, digest) in vectors {
            assert_eq!(hex(md5(text.as_bytes())), digest, "{text:?}");
        }
    }

    #[test]
    fn md5_handles_every_padding_boundary() {
        let vectors = [
            (55, "04364420e25c512fd958a70738aa8f72"),
            (56, "668a72d5ba17f08e62dabcafad6db14b"),
            (57, "693037871c4a9d3d8685018905cb530a"),
            (63, "7dc2ca208106a2f703567bdff99d8981"),
            (64, "c1bb4f81d892b2d57947682aeb252456"),
            (65, "1bc932052302d074bdec39795fe00cf6"),
            (119, "ab347a5f68c8a443cfcddc633f12c24f"),
            (120, "fb98667f98096de92620b64f46e1c5b5"),
            (1000, "cabe45dcc9ae5b66ba86600cca6b8ba8"),
        ];
        for (length, digest) in vectors {
            let data = if length == 1000 { b'a' } else { b'x' };
            assert_eq!(hex(md5(&vec![data; length])), digest, "{length} bytes");
        }
    }

    #[test]
    fn thumbnail_names() {
        // The standard's example URI; the digest is from `md5sum`.
        assert_eq!(
            thumbnail_name("file:///home/jens/photo/me.jpg"),
            "6a6e2356333b14814bde41eb5adb7653.png"
        );
        assert_eq!(
            thumbnail_name(&file_uri(Path::new("/home/jens/photo/me.jpg"))),
            "6a6e2356333b14814bde41eb5adb7653.png"
        );
    }

    #[test]
    fn buckets() {
        assert_eq!(Bucket::for_size(0.0), Bucket::Normal);
        assert_eq!(Bucket::for_size(64.0), Bucket::Normal);
        assert_eq!(Bucket::for_size(128.0), Bucket::Normal);
        assert_eq!(Bucket::for_size(128.5), Bucket::Large);
        assert_eq!(Bucket::for_size(256.0), Bucket::Large);
        assert_eq!(Bucket::for_size(300.0), Bucket::XLarge);
        assert_eq!(Bucket::for_size(512.0), Bucket::XLarge);
        assert_eq!(Bucket::for_size(513.0), Bucket::XXLarge);
        assert_eq!(Bucket::for_size(4000.0), Bucket::XXLarge);
        assert_eq!(
            [
                Bucket::Normal,
                Bucket::Large,
                Bucket::XLarge,
                Bucket::XXLarge
            ]
            .map(Bucket::px),
            [128, 256, 512, 1024]
        );
        assert_eq!(
            Bucket::XLarge.and_larger().collect::<Vec<_>>(),
            vec![Bucket::XLarge, Bucket::XXLarge]
        );
    }

    /// A tiny PNG: signature, a made-up header, the given `tEXt` chunks,
    /// then image data. Checksums are zero; nothing here checks them.
    fn png(texts: &[(&str, &str)]) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&[0; 4]);
        }
        let mut out = PNG_SIGNATURE.to_vec();
        chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
        for (key, value) in texts {
            let mut data = key.as_bytes().to_vec();
            data.push(0);
            data.extend_from_slice(value.as_bytes());
            chunk(&mut out, b"tEXt", &data);
        }
        chunk(&mut out, b"IDAT", &[1, 2, 3]);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn png_text_chunks_are_parsed() {
        let image = png(&[
            ("Thumb::URI", "file:///home/me/a.jpg"),
            ("Thumb::MTime", "1700000123"),
            ("Software", "test"),
        ]);
        assert_eq!(png_mtime(&image), Some(1_700_000_123));
        assert_eq!(png_mtime(&png(&[("Thumb::URI", "x")])), None);
        assert_eq!(png_mtime(&png(&[("Thumb::MTime", "soon")])), None);
        assert_eq!(png_mtime(b"not a png at all"), None);
        assert_eq!(png_mtime(&[]), None);
        // The chunk is cut off by the end of what was read.
        let whole = png(&[("Thumb::MTime", "1700000123")]);
        assert_eq!(png_mtime(&whole[..whole.len() - 30]), Some(1_700_000_123));
        assert_eq!(png_mtime(&whole[..40]), None);
        // Text after the image data is not looked at.
        let mut late = png(&[]);
        late.truncate(late.len() - 12);
        assert_eq!(png_mtime(&late), None);
    }

    struct Cache(PathBuf);

    impl Cache {
        fn new(tag: &str) -> Cache {
            let root = std::env::temp_dir()
                .join(format!("bingux-files-thumbs-{}-{tag}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Cache(root)
        }

        fn put(&self, folder: &str, path: &Path, mtime: &str) -> PathBuf {
            let dir = self.0.join(folder);
            fs::create_dir_all(&dir).unwrap();
            let file = dir.join(thumbnail_name(&file_uri(path)));
            fs::write(&file, png(&[("Thumb::MTime", mtime)])).unwrap();
            file
        }
    }

    impl Drop for Cache {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn at(seconds: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(seconds) + Duration::from_millis(250))
    }

    #[test]
    fn lookup_checks_the_modified_time() {
        let cache = Cache::new("lookup");
        let source = Path::new("/home/me/pictures/cat.jpg");
        let thumb = cache.put("normal", source, "1700000000");
        // The source does not exist: it is never looked at.
        assert_eq!(
            lookup_in(&cache.0, source, at(1_700_000_000), Bucket::Normal),
            Some(thumb.clone())
        );
        assert_eq!(
            lookup_in(&cache.0, source, at(1_700_000_001), Bucket::Normal),
            None
        );
        assert_eq!(
            lookup_in(&cache.0, source, None, Bucket::Normal),
            Some(thumb),
            "no time to compare"
        );
        assert_eq!(
            lookup_in(
                &cache.0,
                Path::new("/home/me/other.jpg"),
                None,
                Bucket::Normal
            ),
            None
        );
    }

    #[test]
    fn lookup_falls_back_to_bigger_buckets_only() {
        let cache = Cache::new("buckets");
        let source = Path::new("/home/me/pictures/dog.png");
        let large = cache.put("large", source, "42");
        assert_eq!(
            lookup_in(&cache.0, source, at(42), Bucket::Normal),
            Some(large.clone())
        );
        assert_eq!(
            lookup_in(&cache.0, source, at(42), Bucket::Large),
            Some(large)
        );
        assert_eq!(
            lookup_in(&cache.0, source, at(42), Bucket::XLarge),
            None,
            "a smaller thumbnail is not stretched"
        );
        // A stale small one does not hide a fresh big one.
        cache.put("normal", source, "1");
        let xl = cache.put("x-large", source, "42");
        assert_eq!(
            lookup_in(&cache.0, source, at(42), Bucket::Normal)
                .map(|p| p.parent().unwrap().to_path_buf()),
            Some(cache.0.join("large"))
        );
        assert_eq!(
            lookup_in(&cache.0, source, at(42), Bucket::XLarge),
            Some(xl)
        );
    }

    #[test]
    fn lookup_with_no_cache_is_none() {
        let cache = Cache::new("empty");
        assert_eq!(
            lookup_in(
                &cache.0.join("missing"),
                Path::new("/a"),
                None,
                Bucket::Normal
            ),
            None
        );
    }

    #[test]
    fn failed_thumbnails() {
        let cache = Cache::new("fail");
        let source = Path::new("/home/me/broken.jpg");
        assert!(!is_failed_in(&cache.0, source, at(7)));
        cache.put("fail/gnome-thumbnail-factory", source, "7");
        assert!(is_failed_in(&cache.0, source, at(7)));
        assert!(
            !is_failed_in(&cache.0, source, at(8)),
            "the file changed since"
        );
        assert!(is_failed_in(&cache.0, source, None));
        assert!(!is_failed_in(
            &cache.0,
            Path::new("/home/me/fine.jpg"),
            at(7)
        ));
    }
}
