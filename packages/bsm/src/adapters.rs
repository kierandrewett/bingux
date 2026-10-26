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
