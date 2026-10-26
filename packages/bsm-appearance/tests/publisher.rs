//! The publisher against a real cache directory, real image files and the real converter.

mod common;

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use bsm_appearance::accounts::{Account, AccountSource};
use bsm_appearance::convert::ImageKind;
use bsm_appearance::profile::{ProfileError, build_user_profile, validate_user_profile, verify_user_profile_files};
use bsm_appearance::publish::{ImageUpdate, PublishError, Published, Publisher, PublisherConfig, Sources, SystemSources};
use bsm_appearance::record::PublishRecord;
use bsm_proto::control::{Appearance, ClockFormat, ColorScheme};
use bsm_proto::state::SessionAvailability;
use common::*;

const UID: u32 = 1000;

struct Env {
    root: tempfile::TempDir,
    publisher: Publisher,
}

impl Env {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("greeter");
        fs::create_dir(&cache).unwrap();
        fs::create_dir(root.path().join("icons")).unwrap();
        let mut config = PublisherConfig::new(&cache, converter());
        config.icon_roots = vec![root.path().join("icons")];
        config.trusted_icon_owners = vec![euid()];
        Self { publisher: Publisher::new(config), root }
    }

    fn cache(&self) -> PathBuf {
        self.root.path().join("greeter")
    }

    fn user_dir(&self) -> PathBuf {
        self.cache().join(UID.to_string())
    }

    fn file(&self, name: &str, bytes: &[u8]) -> fs::File {
        open(&write_file(self.root.path(), name, bytes))
    }

    fn publish(&self, record: &PublishRecord, avatar: ImageUpdate, wallpaper: ImageUpdate) -> Published {
        self.publisher.publish(UID, record, Sources { avatar, wallpaper }).unwrap()
    }
}

fn euid() -> u32 {
    // SAFETY: geteuid cannot fail.
    unsafe { libc::geteuid() }
}

fn record() -> PublishRecord {
    PublishRecord::new(
        Appearance { color_scheme: ColorScheme::Dark, accent: "#3584e4".into(), clock_format: ClockFormat::H24, reduce_motion: false },
        true,
    )
    .unwrap()
}

fn opted_out() -> PublishRecord {
    PublishRecord { show_personal_images: false, ..record() }
}

fn read(path: &str) -> Vec<u8> {
    fs::read(path).unwrap()
}

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp."))
        .collect()
}

fn account(uid: u32, name: &str) -> Account {
    Account {
        uid,
        gid: uid,
        user_name: name.into(),
        display_name: name.into(),
        icon_file: None,
        home: PathBuf::from("/home").join(name),
        shell: "/bin/bash".into(),
        locked: false,
        local: true,
        source: AccountSource::PasswdOnly,
    }
}

#[test]
fn first_publish_writes_sanitised_copies() {
    let env = Env::new();
    let avatar = env.file("face.png", &png_bytes(700, 500, 1));
    let wallpaper = env.file("wall.jpg", &jpeg_bytes(1280, 720, 1));
    let published = env.publish(&record(), ImageUpdate::Set(avatar), ImageUpdate::Set(wallpaper));
    assert!(published.warnings.is_empty(), "{:?}", published.warnings);
    let dir = env.user_dir();
    assert_eq!(published.avatar_path.as_deref(), Some(dir.join("avatar").to_str().unwrap()));
    assert_eq!(published.wallpaper_path.as_deref(), Some(dir.join("wallpaper").to_str().unwrap()));
    assert_eq!(published.wallpaper_blur_path.as_deref(), Some(dir.join("wallpaper-blur").to_str().unwrap()));
    assert_eq!((published.avatar_rev, published.wallpaper_rev), (1, 1));
    let avatar = image::load_from_memory(&read(published.avatar_path.as_ref().unwrap())).unwrap();
    assert_eq!((avatar.width(), avatar.height()), (500, 500));
    let wallpaper = image::load_from_memory(&read(published.wallpaper_path.as_ref().unwrap())).unwrap();
    assert_eq!((wallpaper.width(), wallpaper.height()), (1280, 720));
    assert!(read(published.wallpaper_blur_path.as_ref().unwrap()).len() < read(published.wallpaper_path.as_ref().unwrap()).len());
    // The directory belongs to the publisher, is traversable by the greeter, and files are readable.
    let metadata = fs::metadata(&dir).unwrap();
    assert_eq!(metadata.uid(), euid());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o755);
    for name in ["avatar", "wallpaper", "wallpaper-blur", "profile.json"] {
        assert_eq!(fs::metadata(dir.join(name)).unwrap().permissions().mode() & 0o777, 0o644, "{name}");
    }
    assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
    assert_eq!(published.appearance.accent, "#3584e4");
}

#[test]
fn revisions_move_only_when_content_changes() {
    let env = Env::new();
    let wall_a = jpeg_bytes(800, 600, 1);
    let first = env.publish(&record(), ImageUpdate::Keep, ImageUpdate::Set(env.file("a.jpg", &wall_a)));
    assert_eq!(first.wallpaper_rev, 1);
    let bytes_a = read(first.wallpaper_path.as_ref().unwrap());
    let inode_a = fs::metadata(first.wallpaper_path.as_ref().unwrap()).unwrap().ino();

    // The same source again: nothing is rewritten, the revision stays.
    let again = env.publish(&record(), ImageUpdate::Keep, ImageUpdate::Set(env.file("a2.jpg", &wall_a)));
    assert_eq!(again.wallpaper_rev, 1);
    assert_eq!(fs::metadata(again.wallpaper_path.as_ref().unwrap()).unwrap().ino(), inode_a, "file not replaced");

    // Appearance only, images kept: no revision change for images, the appearance changes.
    let mut changed = record();
    changed.appearance.accent = "#e66100".into();
    changed.appearance.color_scheme = ColorScheme::Light;
    let appearance_only = env.publish(&changed, ImageUpdate::Keep, ImageUpdate::Keep);
    assert_eq!((appearance_only.avatar_rev, appearance_only.wallpaper_rev), (0, 1));
    assert_eq!(appearance_only.appearance.color_scheme, ColorScheme::Light);
    assert_eq!(read(appearance_only.wallpaper_path.as_ref().unwrap()), bytes_a);

    // A different picture bumps it exactly once.
    let second = env.publish(&changed, ImageUpdate::Keep, ImageUpdate::Set(env.file("b.jpg", &jpeg_bytes(800, 600, 9))));
    assert_eq!(second.wallpaper_rev, 2);
    assert_ne!(read(second.wallpaper_path.as_ref().unwrap()), bytes_a);
    // The avatar revision is independent.
    let with_avatar = env.publish(&changed, ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 3))), ImageUpdate::Keep);
    assert_eq!((with_avatar.avatar_rev, with_avatar.wallpaper_rev), (1, 2));
}

#[test]
fn clearing_an_image_removes_it_and_bumps_the_revision() {
    let env = Env::new();
    let published = env.publish(&record(), ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 3))), ImageUpdate::Keep);
    assert_eq!(published.avatar_rev, 1);
    let cleared = env.publish(&record(), ImageUpdate::Clear, ImageUpdate::Clear);
    assert_eq!(cleared.avatar_path, None);
    assert_eq!(cleared.avatar_rev, 2);
    assert_eq!(cleared.wallpaper_rev, 0, "nothing to remove, no bump");
    assert!(!env.user_dir().join("avatar").exists());
    // Setting it again goes on from 2, never back to a revision a client may have cached.
    let again = env.publish(&record(), ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 3))), ImageUpdate::Keep);
    assert_eq!(again.avatar_rev, 3);
}

#[test]
fn opt_out_removes_personal_images_and_keeps_appearance() {
    let env = Env::new();
    let published = env.publish(
        &record(),
        ImageUpdate::Set(env.file("f.png", &png_bytes(128, 128, 1))),
        ImageUpdate::Set(env.file("w.jpg", &jpeg_bytes(640, 480, 1))),
    );
    assert!(published.avatar_path.is_some() && published.wallpaper_path.is_some());

    // Opting out wins even when images are supplied in the same call.
    let out = env.publish(
        &opted_out(),
        ImageUpdate::Set(env.file("f2.png", &png_bytes(128, 128, 2))),
        ImageUpdate::Set(env.file("w2.jpg", &jpeg_bytes(640, 480, 2))),
    );
    assert!(!out.show_personal_images);
    assert_eq!((out.avatar_path.clone(), out.wallpaper_path.clone(), out.wallpaper_blur_path.clone()), (None, None, None));
    assert_eq!((out.avatar_rev, out.wallpaper_rev), (2, 2));
    for name in ["avatar", "wallpaper", "wallpaper-blur"] {
        assert!(!env.user_dir().join(name).exists(), "{name} must be gone from disk");
    }
    assert_eq!(out.appearance.accent, "#3584e4", "colours still publish");
    assert_eq!(env.publisher.load_cached(UID).unwrap(), out);

    // System sources are not adopted for an account that opted out, even when they are valid.
    let uid = euid();
    let snapshot = env.cache().join("alex");
    fs::create_dir(&snapshot).unwrap();
    write_file(&snapshot, "wallpaper", &jpeg_bytes(640, 480, 7));
    env.publisher.publish(uid, &opted_out(), Sources::default()).unwrap();
    let adopted = env.publisher.adopt_system_sources(uid, &SystemSources { user_name: "alex", icon_file: None }).unwrap();
    assert_eq!(adopted.wallpaper_path, None);
    assert!(!env.cache().join(uid.to_string()).join("wallpaper").exists());
    assert!(adopted.warnings.is_empty());

    // Opting back in publishes nothing by itself; the images come with the next publish.
    let back = env.publish(&record(), ImageUpdate::Keep, ImageUpdate::Keep);
    assert!(back.show_personal_images && back.avatar_path.is_none());
    let restored = env.publish(&record(), ImageUpdate::Set(env.file("f3.png", &png_bytes(128, 128, 3))), ImageUpdate::Keep);
    assert_eq!(restored.avatar_rev, 3);
}

#[test]
fn bad_sources_keep_the_last_published_copy() {
    let env = Env::new();
    let good = env.publish(
        &record(),
        ImageUpdate::Set(env.file("f.png", &png_bytes(128, 128, 1))),
        ImageUpdate::Set(env.file("w.jpg", &jpeg_bytes(640, 480, 1))),
    );
    let avatar_bytes = read(good.avatar_path.as_ref().unwrap());
    let wallpaper_bytes = read(good.wallpaper_path.as_ref().unwrap());

    let mut truncated = png_bytes(300, 300, 2);
    truncated.truncate(200);
    let broken = env.publish(
        &record(),
        ImageUpdate::Set(env.file("broken.png", &truncated)),
        ImageUpdate::Set(env.file("script", b"#!/bin/sh\nrm -rf /\n")),
    );
    assert_eq!(broken.warnings.len(), 2, "{:?}", broken.warnings);
    assert!(broken.warnings.iter().all(|warning| warning.kept_previous));
    assert!(broken.warnings.iter().any(|warning| warning.image == ImageKind::Avatar));
    assert_eq!(broken.avatar_path, good.avatar_path);
    assert_eq!((broken.avatar_rev, broken.wallpaper_rev), (1, 1));
    assert_eq!(read(broken.avatar_path.as_ref().unwrap()), avatar_bytes);
    assert_eq!(read(broken.wallpaper_path.as_ref().unwrap()), wallpaper_bytes);

    // A directory handed over as an image, a pixel bomb and an oversized file: all refused, all kept.
    let bomb = {
        use image::codecs::png::{CompressionType, FilterType, PngEncoder};
        use image::{ExtendedColorType, ImageEncoder};
        let mut bytes = Vec::new();
        PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::NoFilter)
            .write_image(&vec![0u8; 9000 * 9000 * 3], 9000, 9000, ExtendedColorType::Rgb8)
            .unwrap();
        bytes
    };
    let hostile = env.publish(
        &record(),
        ImageUpdate::Set(fs::File::open(env.root.path()).unwrap()),
        ImageUpdate::Set(env.file("bomb.png", &bomb)),
    );
    assert_eq!(hostile.warnings.len(), 2, "{:?}", hostile.warnings);
    assert_eq!(read(hostile.avatar_path.as_ref().unwrap()), avatar_bytes);
    assert_eq!(read(hostile.wallpaper_path.as_ref().unwrap()), wallpaper_bytes);
    assert!(leftovers(&env.user_dir()).is_empty());
}

#[test]
fn a_bad_record_changes_nothing() {
    let env = Env::new();
    let good = env.publish(&record(), ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 1))), ImageUpdate::Keep);
    let mut bad = record();
    bad.appearance.accent = "url(javascript:1)".into();
    let error = env
        .publisher
        .publish(UID, &bad, Sources { avatar: ImageUpdate::Set(env.file("g.png", &png_bytes(64, 64, 2))), wallpaper: ImageUpdate::Keep })
        .unwrap_err();
    assert!(matches!(error, PublishError::Record(_)), "{error}");
    assert_eq!(env.publisher.load_cached(UID).unwrap(), good);
}

#[test]
fn readers_never_see_a_partial_file() {
    let env = Env::new();
    let a = jpeg_bytes(900, 600, 1);
    let b = jpeg_bytes(900, 600, 200);
    let first = env.publish(&record(), ImageUpdate::Keep, ImageUpdate::Set(env.file("a.jpg", &a)));
    let path = first.wallpaper_path.clone().unwrap();
    let blur_path = first.wallpaper_blur_path.clone().unwrap();
    let version_a = read(&path);
    let blur_a = read(&blur_path);
    env.publish(&record(), ImageUpdate::Keep, ImageUpdate::Set(env.file("b.jpg", &b)));
    let version_b = read(&path);
    let blur_b = read(&blur_path);
    assert_ne!(version_a, version_b);

    let stop = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(3));
    let readers: Vec<_> = [(path.clone(), vec![version_a.clone(), version_b.clone()]), (blur_path.clone(), vec![blur_a, blur_b])]
        .into_iter()
        .map(|(path, allowed)| {
            let (stop, barrier) = (stop.clone(), barrier.clone());
            thread::spawn(move || {
                barrier.wait();
                let mut reads = 0u32;
                while !stop.load(Ordering::Relaxed) {
                    let bytes = fs::read(&path).expect("the file always exists");
                    assert!(allowed.contains(&bytes), "a reader saw {} bytes that match neither published version", bytes.len());
                    reads += 1;
                }
                reads
            })
        })
        .collect();
    barrier.wait();
    for round in 0..12 {
        let (name, bytes) = if round % 2 == 0 { ("a2.jpg", &a) } else { ("b2.jpg", &b) };
        let published = env.publish(&record(), ImageUpdate::Keep, ImageUpdate::Set(env.file(name, bytes)));
        assert!(published.warnings.is_empty(), "{:?}", published.warnings);
    }
    stop.store(true, Ordering::Relaxed);
    let total: u32 = readers.into_iter().map(|reader| reader.join().unwrap()).sum();
    assert!(total > 50, "the readers really ran concurrently ({total} reads)");
    assert!(leftovers(&env.user_dir()).is_empty());
}

#[test]
fn concurrent_publishers_do_not_collide() {
    let env = Arc::new(Env::new());
    let wallpaper = jpeg_bytes(640, 480, 5);
    let handles: Vec<_> = (0..4)
        .map(|index| {
            let env = env.clone();
            let wallpaper = wallpaper.clone();
            thread::spawn(move || {
                let file = env.file(&format!("w{index}.jpg"), &wallpaper);
                env.publisher.publish(UID, &record(), Sources { avatar: ImageUpdate::Keep, wallpaper: ImageUpdate::Set(file) }).unwrap()
            })
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap().warnings.is_empty());
    }
    let cached = env.publisher.load_cached(UID).unwrap();
    assert_eq!(cached.wallpaper_rev, 1, "four identical publishes are one change");
    assert!(leftovers(&env.user_dir()).is_empty());
}

#[test]
fn an_untrusted_account_directory_is_refused() {
    let env = Env::new();
    // A symlink where our directory should be (a planted entry in a shared cache root).
    let target = env.root.path().join("elsewhere");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, env.user_dir()).unwrap();
    let error = env.publisher.publish(UID, &record(), Sources::default()).unwrap_err();
    assert!(matches!(error, PublishError::UntrustedDirectory { .. }), "{error}");
    assert!(fs::read_dir(&target).unwrap().next().is_none(), "nothing was written through the symlink");
    assert!(env.publisher.load_cached(UID).is_none());
}

#[test]
fn a_loose_directory_mode_is_tightened() {
    let env = Env::new();
    fs::create_dir(env.user_dir()).unwrap();
    fs::set_permissions(env.user_dir(), fs::Permissions::from_mode(0o777)).unwrap();
    env.publisher.publish(UID, &record(), Sources::default()).unwrap();
    assert_eq!(fs::metadata(env.user_dir()).unwrap().permissions().mode() & 0o777, 0o755);
}

#[test]
fn a_restart_reads_the_cache_back() {
    let env = Env::new();
    assert!(env.publisher.load_cached(UID).is_none(), "nothing published yet");
    let published = env.publish(
        &record(),
        ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 1))),
        ImageUpdate::Set(env.file("w.jpg", &jpeg_bytes(320, 200, 1))),
    );
    let restarted = Publisher::new(env.publisher.config().clone());
    assert_eq!(restarted.load_cached(UID), Some(published));
    // A file deleted behind our back is not advertised.
    fs::remove_file(env.user_dir().join("avatar")).unwrap();
    assert_eq!(restarted.load_cached(UID).unwrap().avatar_path, None);
}

#[test]
fn a_damaged_profile_never_reuses_a_revision() {
    let env = Env::new();
    env.publish(&record(), ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 1))), ImageUpdate::Keep);
    fs::write(env.user_dir().join("profile.json"), b"{ this is not json").unwrap();
    let after = env.publish(&record(), ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 1))), ImageUpdate::Keep);
    assert!(after.avatar_rev > 1_000_000, "time based base after damage, got {}", after.avatar_rev);
    let newer = fs::read_to_string(env.user_dir().join("profile.json")).unwrap().replace("\"version\": 1", "\"version\": 99");
    fs::write(env.user_dir().join("profile.json"), newer).unwrap();
    let error = env.publisher.publish(UID, &record(), Sources::default()).unwrap_err();
    assert!(matches!(error, PublishError::UnsupportedState(99)), "{error}");
}

#[test]
fn account_icons_are_adopted_only_from_trusted_files() {
    let env = Env::new();
    let icons = env.root.path().join("icons");
    write_file(&icons, "alex", &png_bytes(256, 256, 4));
    let published = env
        .publisher
        .adopt_system_sources(UID, &SystemSources { user_name: "alex", icon_file: Some(&icons.join("alex")) })
        .unwrap();
    assert!(published.warnings.is_empty(), "{:?}", published.warnings);
    assert!(published.avatar_path.is_some());
    assert_eq!(published.avatar_rev, 1);
    // Unchanged icon: same revision.
    let again = env
        .publisher
        .adopt_system_sources(UID, &SystemSources { user_name: "alex", icon_file: Some(&icons.join("alex")) })
        .unwrap();
    assert_eq!(again.avatar_rev, 1);

    // A symlink in the icons directory, a path outside the roots and a file not owned by a trusted
    // uid are all refused and leave the published copy alone.
    std::os::unix::fs::symlink("/etc/hostname", icons.join("link")).unwrap();
    let outside = write_file(env.root.path(), "outside.png", &png_bytes(64, 64, 1));
    for icon in [icons.join("link"), outside] {
        let result = env.publisher.adopt_system_sources(UID, &SystemSources { user_name: "alex", icon_file: Some(&icon) }).unwrap();
        assert_eq!(result.warnings.len(), 1, "{icon:?}: {:?}", result.warnings);
        assert!(result.warnings[0].kept_previous);
        assert_eq!(result.avatar_rev, 1);
    }
    let mut strict = env.publisher.config().clone();
    strict.trusted_icon_owners = vec![euid() + 1];
    let strict = Publisher::new(strict);
    let result = strict.adopt_system_sources(UID, &SystemSources { user_name: "alex", icon_file: Some(&icons.join("alex")) }).unwrap();
    assert_eq!(result.warnings.len(), 1);

    // The icon going away removes the copy that came from it.
    fs::remove_file(icons.join("alex")).unwrap();
    let gone = env
        .publisher
        .adopt_system_sources(UID, &SystemSources { user_name: "alex", icon_file: Some(&icons.join("alex")) })
        .unwrap();
    assert_eq!((gone.avatar_path, gone.avatar_rev), (None, 2));
}

#[test]
fn snapshots_are_adopted_under_the_documented_rule() {
    let env = Env::new();
    let snapshot = env.cache().join("alex");
    fs::create_dir(&snapshot).unwrap();
    write_file(&snapshot, "wallpaper", &jpeg_bytes(1000, 700, 6));
    let sources = SystemSources { user_name: "alex", icon_file: None };

    // uid == the directory owner: adopted, and rewritten as a sanitised copy under <uid>.
    let uid = euid();
    let published = env.publisher.adopt_system_sources(uid, &sources).unwrap();
    assert!(published.warnings.is_empty(), "{:?}", published.warnings);
    let path = published.wallpaper_path.clone().unwrap();
    assert_eq!(path, env.cache().join(uid.to_string()).join("wallpaper").to_str().unwrap());
    assert_eq!(published.wallpaper_rev, 1);
    assert!(published.wallpaper_blur_path.is_some());
    assert_ne!(read(&path), fs::read(snapshot.join("wallpaper")).unwrap(), "a re-encoded copy, not the user file");

    // The same snapshot again: no new revision.
    assert_eq!(env.publisher.adopt_system_sources(uid, &sources).unwrap().wallpaper_rev, 1);

    // A snapshot that belongs to someone else's uid is not adopted.
    let stranger = env.publisher.adopt_system_sources(uid + 1, &sources).unwrap();
    assert_eq!(stranger.wallpaper_path, None);
    assert_eq!(stranger.warnings.len(), 1, "{:?}", stranger.warnings);

    // A symlinked snapshot file, a symlinked snapshot directory and numeric names are refused.
    fs::remove_file(snapshot.join("wallpaper")).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", snapshot.join("wallpaper")).unwrap();
    let linked = env.publisher.adopt_system_sources(uid + 2, &sources).unwrap();
    assert_eq!((linked.wallpaper_path, linked.warnings.len()), (None, 1));
    std::os::unix::fs::symlink(&snapshot, env.cache().join("mallory")).unwrap();
    let dir_link = env
        .publisher
        .adopt_system_sources(uid + 3, &SystemSources { user_name: "mallory", icon_file: None })
        .unwrap();
    assert_eq!((dir_link.wallpaper_path, dir_link.warnings.len()), (None, 1));
    let numeric = env.publisher.adopt_system_sources(uid + 4, &SystemSources { user_name: "1000", icon_file: None }).unwrap();
    assert_eq!((numeric.wallpaper_path, numeric.warnings.len()), (None, 1));
    let traversal = env.publisher.adopt_system_sources(uid + 5, &SystemSources { user_name: "../alex", icon_file: None }).unwrap();
    assert_eq!((traversal.wallpaper_path, traversal.warnings.len()), (None, 1));
}

#[test]
fn a_published_wallpaper_is_not_replaced_by_a_system_source() {
    let env = Env::new();
    let uid = euid();
    let snapshot = env.cache().join("alex");
    fs::create_dir(&snapshot).unwrap();
    write_file(&snapshot, "wallpaper", &jpeg_bytes(800, 600, 1));
    let own = env
        .publisher
        .publish(uid, &record(), Sources { avatar: ImageUpdate::Keep, wallpaper: ImageUpdate::Set(env.file("own.jpg", &jpeg_bytes(800, 600, 99))) })
        .unwrap();
    let bytes = read(own.wallpaper_path.as_ref().unwrap());
    let adopted = env.publisher.adopt_system_sources(uid, &SystemSources { user_name: "alex", icon_file: None }).unwrap();
    assert_eq!(adopted.wallpaper_rev, own.wallpaper_rev);
    assert_eq!(read(adopted.wallpaper_path.as_ref().unwrap()), bytes);
}

#[test]
fn greeter_profiles_point_only_at_published_files() {
    let env = Env::new();
    let uid = euid();
    let published = env
        .publisher
        .publish(
            uid,
            &record(),
            Sources {
                avatar: ImageUpdate::Set(env.file("f.png", &png_bytes(64, 64, 1))),
                wallpaper: ImageUpdate::Set(env.file("w.jpg", &jpeg_bytes(320, 200, 1))),
            },
        )
        .unwrap();
    let profile = build_user_profile(&account(uid, "alex"), Some(&published), SessionAvailability::NotSignedIn, None);
    assert_eq!(validate_user_profile(&profile, &env.cache()), Ok(()));
    assert_eq!(verify_user_profile_files(&profile, &env.cache()), Ok(()));
    assert_eq!(profile.avatar_rev, 1);

    // The avatar replaced by a symlink to a user file: the disk check refuses it.
    let avatar = env.cache().join(uid.to_string()).join("avatar");
    fs::remove_file(&avatar).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", &avatar).unwrap();
    assert!(matches!(verify_user_profile_files(&profile, &env.cache()), Err(ProfileError::File { field: "avatar_path", .. })));
    // A path into someone else's directory fails the lexical check without touching the disk.
    let mut stolen = profile.clone();
    stolen.avatar_path = Some(env.cache().join("1").join("avatar").to_string_lossy().into_owned());
    assert!(matches!(validate_user_profile(&stolen, &env.cache()), Err(ProfileError::Path { .. })));
}
