//! Per-account greeter snapshots.
//!
//! The greeter runs as the greeter account and cannot read home directories,
//! so each user session exports what the login screen needs into
//! `/var/lib/bingux/greeter/<user>/`. The parent is a sticky, world-writable
//! directory from tmpfiles; a snapshot counts only when its directory is owned
//! by the account it names, so one user cannot dress up another's login tile.

use image::{
    ExtendedColorType, ImageEncoder, ImageReader, codecs::jpeg::JpegEncoder,
    codecs::png::PngEncoder, imageops,
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

pub const SNAPSHOT_ROOT: &str = "/var/lib/bingux/greeter";
const WALLPAPER_FILE: &str = "wallpaper";
const SESSION_FILE: &str = "session.json";
const MAX_WALLPAPER_BYTES: u64 = 64 * 1024 * 1024;
const BACKGROUND_DROPIN: &str = "bingux-background.lua";
/// Shown where the image is missing or still decoding; the shell's base tone.
const BACKGROUND_COLOR: &str = "#161616";
/// Gaussian blur applied to the wallpaper behind the login screen, as the
/// standard deviation in pixels of the source image. Used by the export and by
/// the `blur-wallpaper` subcommand that builds the packaged twin.
pub const WALLPAPER_BLUR_RADIUS: f32 = 32.0;
/// Brightness of the blurred wallpaper: 1.0 keeps the picture, 0.0 is black.
pub const WALLPAPER_DIM: f32 = 0.65;
/// Name of the blurred copy written beside the snapshot wallpaper; the path to
/// use for a blurred backdrop.
pub const BLURRED_WALLPAPER_FILE: &str = "wallpaper-blur";
/// JPEG quality of the blurred copy; blur hides detail, so this stays modest.
const BLURRED_JPEG_QUALITY: u8 = 88;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Readable copy of the account's wallpaper, or empty.
    #[serde(default)]
    pub wallpaper: String,
}

/// The wallpaper the user's own Bingux configuration resolves to, through the
/// bingux-config CLI so the two never disagree. Empty when bingux-config is
/// absent or nothing is configured.
pub fn configured_wallpaper() -> String {
    let configured = command_stdout(&["get", "desktop.wallpaper"]);
    if !configured.is_empty() && Path::new(&configured).is_absolute() {
        return configured;
    }
    let fallback = command_stdout(&["wallpaper"]);
    serde_json::from_str::<serde_json::Value>(&fallback)
        .ok()
        .and_then(|value| value.get("wallpaper")?.as_str().map(str::to_string))
        .and_then(|uri| uri.strip_prefix("file://").map(str::to_string))
        .filter(|path| Path::new(path).is_absolute())
        .unwrap_or_default()
}

/// `privacy.show_personal_images` in the user's Bingux configuration; on unless
/// it says `false`.
pub fn show_personal_images() -> bool {
    command_stdout(&["get", "privacy.show_personal_images"]) != "false"
}

fn command_stdout(arguments: &[&str]) -> String {
    Command::new("bingux-config")
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Writes the calling user's snapshot. `root` is `SNAPSHOT_ROOT` in production.
pub fn export(root: &Path, user: &str, wallpaper: &str) -> io::Result<Snapshot> {
    let directory = root.join(user);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    let target = directory.join(WALLPAPER_FILE);
    let mut snapshot = Snapshot::default();
    let source = Path::new(wallpaper);
    if !wallpaper.is_empty()
        && source.is_file()
        && fs::metadata(source)?.len() <= MAX_WALLPAPER_BYTES
    {
        let temporary = directory.join(".wallpaper.tmp");
        fs::copy(source, &temporary)?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
        fs::rename(&temporary, &target)?;
        snapshot.wallpaper = target.display().to_string();
    } else if target.exists() {
        fs::remove_file(&target)?;
    }
    // The blurred twin sits beside the snapshot wallpaper. A failure to blur
    // (an undecodable or unusual format) keeps the plain wallpaper, so it
    // never fails the export; a stale twin is removed rather than shown.
    let blurred = directory.join(BLURRED_WALLPAPER_FILE);
    if snapshot.wallpaper.is_empty() {
        remove_if_exists(&blurred)?;
    } else if let Err(error) = write_blurred(&target, &blurred, BlurredFormat::Jpeg) {
        eprintln!("bingux-session: could not blur the greeter wallpaper: {error}");
        remove_if_exists(&blurred)?;
    }
    let temporary = directory.join(".session.json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(&snapshot)? + "\n")?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
    fs::rename(&temporary, directory.join(SESSION_FILE))?;
    Ok(snapshot)
}

/// Encoding of a blurred copy. The greeter's snapshot copy has no extension,
/// so the export asks for JPEG explicitly rather than inferring it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlurredFormat {
    Jpeg,
    Png,
}

/// The encoding for a blurred copy written to `path`, judged by its extension.
pub fn blurred_format(path: &Path) -> Option<BlurredFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some(BlurredFormat::Jpeg),
        "png" => Some(BlurredFormat::Png),
        _ => None,
    }
}

/// Writes a pre-blurred, dimmed copy of the image at `source` to `target`.
/// The file is replaced in one rename, so a reader never sees half of it, and
/// it is world-readable like the snapshot wallpaper it derives from.
pub fn write_blurred(source: &Path, target: &Path, format: BlurredFormat) -> io::Result<()> {
    let decoded = ImageReader::open(source)?
        .with_guessed_format()?
        .decode()
        .map_err(io::Error::other)?;
    let mut picture = imageops::blur(&decoded.to_rgb8(), WALLPAPER_BLUR_RADIUS);
    for pixel in picture.pixels_mut() {
        for channel in &mut pixel.0 {
            *channel = (f32::from(*channel) * WALLPAPER_DIM).round() as u8;
        }
    }
    let (width, height) = picture.dimensions();
    let mut bytes = Vec::new();
    let encoded = match format {
        BlurredFormat::Jpeg => JpegEncoder::new_with_quality(&mut bytes, BLURRED_JPEG_QUALITY)
            .write_image(picture.as_raw(), width, height, ExtendedColorType::Rgb8),
        BlurredFormat::Png => PngEncoder::new(&mut bytes).write_image(
            picture.as_raw(),
            width,
            height,
            ExtendedColorType::Rgb8,
        ),
    };
    encoded.map_err(io::Error::other)?;
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = target.with_file_name(format!(".{name}.tmp"));
    fs::write(&temporary, bytes)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o644))?;
    fs::rename(&temporary, target)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Reads a snapshot the greeter may trust for `user` with `uid`.
pub fn read(root: &Path, user: &str, uid: u32) -> Snapshot {
    let directory = root.join(user);
    let Ok(metadata) = fs::metadata(&directory) else {
        return Snapshot::default();
    };
    if !metadata.is_dir() || metadata.uid() != uid {
        return Snapshot::default();
    }
    let mut snapshot: Snapshot = fs::read_to_string(directory.join(SESSION_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let wallpaper = directory.join(WALLPAPER_FILE);
    let trusted = snapshot.wallpaper == wallpaper.display().to_string()
        && fs::metadata(&wallpaper).is_ok_and(|meta| meta.is_file() && meta.uid() == uid);
    if !trusted {
        snapshot.wallpaper = String::new();
    }
    snapshot
}

/// Writes the Gnoblin drop-in that paints the account's wallpaper under every
/// surface, so the compositor's first frame after login or unlock is the
/// picture and not black. `config_home` is `$XDG_CONFIG_HOME`; the installer's
/// init.lua loads `gnoblin/conf.d/**/*.lua`. `image` is the path from
/// `handover_image`; an empty image keeps only the colour.
pub fn write_background_dropin(config_home: &Path, wallpaper: &str) -> io::Result<PathBuf> {
    let directory = config_home.join("gnoblin/conf.d");
    fs::create_dir_all(&directory)?;
    let text = format!(
        "-- Generated by `bingux-session export`; do not edit. Gnoblin paints this\n\
         -- under every surface, so the first frame after login or unlock is the\n\
         -- Bingux wallpaper rather than black. It names the greeter's blurred copy\n\
         -- so the first frame matches the login screen; the desktop wallpaper then\n\
         -- fades in sharp.\n{}",
        greeter_background_lua(wallpaper)
    );
    let target = directory.join(BACKGROUND_DROPIN);
    let temporary = directory.join(".bingux-background.lua.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, &target)?;
    Ok(target)
}

/// The image the user's compositor background paints: the blurred twin beside
/// the account's snapshot wallpaper, which is the picture the greeter left on
/// screen, or the plain snapshot copy when no twin was written. `directory` is
/// `root/<user>`. The desktop wallpaper crossfades in sharp over it.
pub fn handover_image(directory: &Path) -> String {
    greeter_image(&directory.join(WALLPAPER_FILE).display().to_string())
}

/// The image the login screen should paint: the blurred, dimmed twin written
/// beside the account's snapshot wallpaper when it exists, else the plain
/// wallpaper. The compositor background and the greeter client both use this
/// path, so software-rendered machines show the same blur.
pub fn greeter_image(wallpaper: &str) -> String {
    if let Some(twin) = Path::new(wallpaper)
        .parent()
        .map(|directory| directory.join(BLURRED_WALLPAPER_FILE))
        .filter(|twin| twin.is_file())
    {
        return twin.display().to_string();
    }
    wallpaper.to_string()
}

/// Lua for the greeter compositor: the remembered account's exported
/// wallpaper as `compositor.background`, so the login screen's compositor
/// paints it before the greeter client maps. With no remembered account the
/// first listed account's wallpaper is used; with none at all, the colour.
pub fn greeter_background_lua(wallpaper: &str) -> String {
    let image = if wallpaper.is_empty() || !Path::new(wallpaper).is_file() {
        String::new()
    } else {
        format!(
            ", image = \"{}\"",
            wallpaper.replace('\\', "\\\\").replace('"', "\\\"")
        )
    };
    format!(
        "gnoblin.configure {{\n\
         \x20   compositor = {{\n\
         \x20       background = {{ color = \"{BACKGROUND_COLOR}\"{image} }},\n\
         \x20   }},\n\
         }}\n"
    )
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

pub fn root() -> PathBuf {
    std::env::var_os("BINGUX_GREETER_SNAPSHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SNAPSHOT_ROOT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_then_read_round_trips_and_rejects_foreign_owners() {
        let root =
            std::env::temp_dir().join(format!("bingux-session-snapshot-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let image = root.join("source.png");
        fs::write(&image, b"not really a png").unwrap();
        // SAFETY: tests in this module run on one thread.
        let uid = unsafe { libc::getuid() };
        let exported = export(&root, "ada", image.to_str().unwrap()).unwrap();
        assert_eq!(
            exported.wallpaper,
            root.join("ada/wallpaper").display().to_string()
        );
        assert_eq!(read(&root, "ada", uid), exported);
        assert_eq!(read(&root, "ada", uid + 1), Snapshot::default());
        assert_eq!(read(&root, "nobody", uid), Snapshot::default());
        let cleared = export(&root, "ada", "").unwrap();
        assert_eq!(cleared.wallpaper, "");
        assert!(!root.join("ada/wallpaper").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn greeter_image_prefers_the_blurred_twin_beside_the_snapshot() {
        let root = std::env::temp_dir().join(format!("bingux-session-twin-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let plain = root.join("wallpaper");
        fs::write(&plain, b"plain").unwrap();
        let plain_text = plain.display().to_string();
        assert_eq!(greeter_image(&plain_text), plain_text);
        let twin = root.join(BLURRED_WALLPAPER_FILE);
        fs::write(&twin, b"blur").unwrap();
        assert_eq!(greeter_image(&plain_text), twin.display().to_string());
        assert_eq!(greeter_image(""), "");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn background_dropin_quotes_the_path_and_drops_missing_images() {
        let root =
            std::env::temp_dir().join(format!("bingux-session-dropin-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let image = root.join("odd \"name\".png");
        fs::write(&image, b"png").unwrap();
        let target = write_background_dropin(&root, image.to_str().unwrap()).unwrap();
        let text = fs::read_to_string(&target).unwrap();
        assert!(text.contains("gnoblin.configure {"));
        assert!(text.contains("color = \"#161616\""));
        assert!(text.contains(&format!(
            "image = \"{}/odd \\\"name\\\".png\"",
            root.display()
        )));
        let cleared = write_background_dropin(&root, "/does/not/exist.png").unwrap();
        assert!(!fs::read_to_string(cleared).unwrap().contains("image ="));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn handover_image_names_the_twin_the_greeter_shows() {
        let root = std::env::temp_dir().join(format!("bingux-session-handover-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let plain = root.join("wallpaper");
        fs::write(&plain, b"plain").unwrap();
        assert_eq!(handover_image(&root), plain.display().to_string());
        let twin = root.join(BLURRED_WALLPAPER_FILE);
        fs::write(&twin, b"blur").unwrap();
        assert_eq!(handover_image(&root), twin.display().to_string());
        assert_eq!(greeter_image(&plain.display().to_string()), handover_image(&root));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_writes_a_dimmed_blur_beside_the_wallpaper() {
        let root = std::env::temp_dir().join(format!("bingux-session-blur-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let image = root.join("white.png");
        image::RgbImage::from_pixel(64, 64, image::Rgb([255, 255, 255]))
            .save(&image)
            .unwrap();
        export(&root, "ada", image.to_str().unwrap()).unwrap();
        let blurred = ImageReader::open(root.join("ada").join(BLURRED_WALLPAPER_FILE))
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap()
            .to_rgb8();
        // A flat picture stays flat under the blur, so only the dim shows.
        let expected = (255.0 * WALLPAPER_DIM).round() as i32;
        let centre = blurred.get_pixel(32, 32).0;
        assert!((i32::from(centre[0]) - expected).abs() <= 3, "{centre:?}");
        let cleared = export(&root, "ada", "").unwrap();
        assert_eq!(cleared.wallpaper, "");
        assert!(!root.join("ada").join(BLURRED_WALLPAPER_FILE).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn blurred_format_follows_the_extension() {
        assert_eq!(
            blurred_format(Path::new("default-blur.jpg")),
            Some(BlurredFormat::Jpeg)
        );
        assert_eq!(blurred_format(Path::new("x.PNG")), Some(BlurredFormat::Png));
        assert_eq!(blurred_format(Path::new(BLURRED_WALLPAPER_FILE)), None);
    }
}
