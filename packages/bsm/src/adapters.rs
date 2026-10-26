//! The real account source and appearance backend, both through `bsm-appearance`.

use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use bsm_appearance::accounts::{AccountPolicy, list_accounts};
use bsm_appearance::publish::{ImageUpdate, Published, Publisher, PublisherConfig, Sources, SystemSources};
use bsm_appearance::record::PublishRecord;
use bsm_manager::core::{AccountEntry, FdHandle, PublishRequest, PublishedView};
use bsm_manager::runtime::{AccountSource, AppearanceBackend};
use bsm_proto::error::{BsmError, ErrorCode};

pub fn view(p: &Published) -> PublishedView {
    PublishedView {
        appearance: p.appearance.clone(),
        avatar_path: p.avatar_path.clone(),
        avatar_rev: p.avatar_rev,
        wallpaper_path: p.wallpaper_path.clone(),
        wallpaper_blur_path: p.wallpaper_blur_path.clone(),
        wallpaper_rev: p.wallpaper_rev,
        show_personal_images: p.show_personal_images,
    }
}

/// Eligible accounts with whatever has been published for them. Accounts that are not
/// signed in get their avatar from AccountsService (opened by bsm itself, below trusted
/// roots) and their wallpaper from `bingux-session export` snapshots, never from a user path.
pub struct SystemAccounts {
    pub policy: AccountPolicy,
    pub publisher: Arc<Publisher>,
}

impl AccountSource for SystemAccounts {
    fn list(&self) -> Result<Vec<AccountEntry>, BsmError> {
        let accounts =
            list_accounts(&self.policy).map_err(|e| BsmError::new(ErrorCode::Internal, format!("cannot list accounts: {e}")))?;
        let mut out = Vec::with_capacity(accounts.len());
        for a in accounts {
            let mut published = self.publisher.load_cached(a.uid);
            if published.is_none() {
                let sources = SystemSources { user_name: &a.user_name, icon_file: a.icon_file.as_deref() };
                match self.publisher.adopt_system_sources(a.uid, &sources) {
                    Ok(p) => published = Some(p),
                    Err(e) => tracing::warn!(uid = a.uid, "cannot adopt the account's own images: {e}"),
                }
            }
            out.push(AccountEntry {
                uid: a.uid,
                user_name: a.user_name,
                display_name: a.display_name,
                published: published.as_ref().map(view),
            });
        }
        Ok(out)
    }
}

/// Publishes an appearance record and the images the caller handed over as descriptors.
pub struct PublisherBackend {
    pub publisher: Arc<Publisher>,
}

fn image(update: Option<FdHandle>) -> ImageUpdate {
    match update {
        None => ImageUpdate::Keep,
        Some(h) => match h.0.try_clone() {
            Ok(fd) => ImageUpdate::Set(File::from(fd)),
            Err(e) => {
                tracing::warn!("cannot duplicate an image descriptor: {e}");
                ImageUpdate::Keep
            }
        },
    }
}

impl AppearanceBackend for PublisherBackend {
    fn publish(&self, uid: u32, request: PublishRequest) -> Result<PublishedView, BsmError> {
        let record = PublishRecord::new(request.appearance, request.show_personal_images)
            .map_err(|e| BsmError::new(ErrorCode::InvalidArgument, e.to_string()))?;
        let sources = Sources { avatar: image(request.avatar), wallpaper: image(request.wallpaper) };
        let published = self
            .publisher
            .publish(uid, &record, sources)
            .map_err(|e| BsmError::new(ErrorCode::Internal, format!("cannot publish appearance: {e}")))?;
        for w in &published.warnings {
            tracing::warn!(uid, "image not published: {}", w.error);
        }
        Ok(view(&published))
    }
}

pub fn system_publisher(cache_dir: Option<PathBuf>) -> Arc<Publisher> {
    let mut cfg = PublisherConfig::system();
    if let Some(d) = cache_dir {
        cfg.cache_dir = d;
    }
    Arc::new(Publisher::new(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bsm_appearance::convert::Converter;
    use bsm_proto::control::{Appearance, ColorScheme};

    fn fixture() -> (tempfile::TempDir, AccountPolicy, Arc<Publisher>) {
        let root = tempfile::tempdir().unwrap();
        let etc = root.path().join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::write(
            etc.join("passwd"),
            "root:x:0:0:root:/root:/bin/bash\n\
             alice:x:1000:1000:Alice Example:/home/alice:/bin/bash\n\
             bob:x:1001:1001:Bob:/home/bob:/bin/zsh\n\
             daemon:x:2:2:daemon:/:/usr/sbin/nologin\n\
             svc:x:1500:1500:service:/srv:/usr/sbin/nologin\n",
        )
        .unwrap();
        let policy = AccountPolicy::under(root.path());
        let cache = root.path().join("cache");
        // Created by the packaging (tmpfiles.d) and owned by the bsm account.
        std::fs::create_dir_all(&cache).unwrap();
        let publisher = Arc::new(Publisher::new(PublisherConfig::new(&cache, Converter::new("/nonexistent/bsm-imgconv"))));
        (root, policy, publisher)
    }

    #[test]
    fn only_eligible_accounts_are_listed() {
        let (_root, policy, publisher) = fixture();
        let list = SystemAccounts { policy, publisher }.list().unwrap();
        let names: Vec<_> = list.iter().map(|a| (a.uid, a.user_name.as_str(), a.display_name.as_str())).collect();
        assert_eq!(names, vec![(1000, "alice", "Alice Example"), (1001, "bob", "Bob")]);
    }

    #[test]
    fn publishing_validates_and_the_result_is_what_the_greeter_shows() {
        let (_root, policy, publisher) = fixture();
        let backend = PublisherBackend { publisher: publisher.clone() };
        let request = |accent: &str| PublishRequest {
            appearance: Appearance { color_scheme: ColorScheme::Dark, accent: accent.into(), ..Appearance::default() },
            show_personal_images: false,
            wallpaper: None,
            avatar: None,
        };
        let e = backend.publish(1000, request("not a colour")).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArgument);
        let v = backend.publish(1000, request("#3584E4")).unwrap();
        assert!(!v.show_personal_images);
        assert_eq!(v.appearance.accent.len(), 7);
        // A restart finds it again, through the account source.
        let list = SystemAccounts { policy, publisher }.list().unwrap();
        let alice = list.iter().find(|a| a.uid == 1000).unwrap();
        assert_eq!(alice.published.as_ref().unwrap().appearance, v.appearance);
    }
}
