//! `bingux-media --snapshot DIR [PAGE,...]`: the window to PNG without a
//! compositor, through `bingux_ui::snapshot`.
//!
//! Each PAGE (`songs`, `albums`, `artists`, `videos`, `queue`; all of them by
//! default) is written as `media-PAGE.png`. `BINGUX_MEDIA_SNAPSHOT_PLAY=FILE`
//! plays a file first (with `BINGUX_MEDIA_AO=null` for no sound), so the bar
//! shows; `BINGUX_MEDIA_STAGE=1` opens the full player; `BINGUX_MEDIA_SNAPSHOT_SIZE="WxH"`
//! picks the size (1100x760 by default).

use std::sync::Arc;
use std::time::Duration;

use bingux_ui::nav::provide_router;
use bingux_ui::prelude::*;
use bingux_ui::snapshot::Snapshot;
use bingux_ui::style::{Paint, Tw, bg, h_full, w_full};
use freya::prelude::*;

use crate::{SCHEME, app, engine, routes};

pub fn run(dir: &str, pages: Option<&str>) {
    let size = std::env::var("BINGUX_MEDIA_SNAPSHOT_SIZE")
        .ok()
        .and_then(|text| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
        .unwrap_or((1100.0, 760.0));
    if let Ok(file) = std::env::var("BINGUX_MEDIA_SNAPSHOT_PLAY") {
        engine::engine().play_paths(&[file.into()]);
    }
    // Let the scan and the player settle.
    std::thread::sleep(Duration::from_millis(1500));
    let pages = pages.unwrap_or("songs,albums,artists,videos,queue");
    for page in pages.split(',') {
        let location = Location::new(SCHEME).join(page);
        let mut shot = Snapshot::new(
            move || {
                let tokens = bingux_ui::theme::load_tokens();
                provide_router(Arc::new(routes()), &location);
                Element::from(
                    rect()
                        .tw(&tokens, &[w_full(), h_full(), bg(Paint::Bg)])
                        .child(app::App),
                )
            },
            size,
        );
        shot.settle(Duration::from_millis(1500));
        let path = format!("{dir}/media-{page}.png");
        shot.save(&path);
        println!("[snapshot] wrote {path}");
    }
}
