//! How a size and a speed read, what the list keeps, and what a private
//! window keeps out of it.

use bingux_ui::web_view::BrowserEvent;
use bingux_ui::web_view::requests::{DownloadInfo, DownloadState};
use bingux_ui::web_view::test_double::{Call, TestDoubleBackend};

use super::*;
use crate::browser::testing::{browser, go};
use crate::browser::{Browser, Options};

use bingux_ui::web_view::DownloadId;

fn info(id: u64, state: DownloadState, received: u64, total: Option<u64>) -> DownloadInfo {
    DownloadInfo {
        id: DownloadId(id),
        url: format!("https://example.com/{id}"),
        filename: format!("file-{id}.bin"),
        path: None,
        mime: "application/octet-stream".to_string(),
        state,
        received,
        total,
        speed: 1_000_000,
        container: 0,
        private: false,
    }
}

#[test]
fn sizes_read_in_powers_of_a_thousand() {
    crate::locale::register();
    assert_eq!(Transfer::size(0), "0\u{a0}B");
    assert_eq!(Transfer::size(999), "999\u{a0}B");
    assert_eq!(Transfer::size(1_000), "1\u{a0}kB");
    assert_eq!(Transfer::size(1_234_567), "1.2\u{a0}MB");
    assert_eq!(Transfer::size(84_000_000), "84\u{a0}MB");
    assert_eq!(Transfer::size(840_000_000), "840\u{a0}MB");
    assert_eq!(Transfer::size(2_500_000_000_000), "2.5\u{a0}TB");
}

#[test]
fn time_left_rounds_up_and_needs_a_speed() {
    crate::locale::register();
    assert_eq!(
        Transfer::time_left(5_000_000, 1_000_000).as_deref(),
        Some("5 sec")
    );
    assert_eq!(
        Transfer::time_left(61_000_000, 1_000_000).as_deref(),
        Some("2 min")
    );
    assert_eq!(
        Transfer::time_left(7_300_000_000, 1_000_000).as_deref(),
        Some("3 hr")
    );
    assert_eq!(Transfer::time_left(1, 0), None);
}

#[test]
fn a_running_download_says_how_far_how_fast_and_how_long() {
    crate::locale::register();
    let running = info(1, DownloadState::Running, 3_200_000, Some(8_400_000));
    assert_eq!(
        Transfer(&running).status_line(),
        "3.2\u{a0}MB\u{a0}of\u{a0}8.4\u{a0}MB \u{b7} 1\u{a0}MB/s \u{b7} 6\u{a0}sec\u{a0}left"
    );
    assert_eq!(
        Transfer(&running).fraction().map(|f| (f * 100.0).round()),
        Some(38.0)
    );
    let unknown = info(2, DownloadState::Running, 500, None);
    assert_eq!(Transfer(&unknown).fraction(), None);
    assert!(Transfer(&unknown).status_line().starts_with("500\u{a0}B"));
}

#[test]
fn the_other_states_read_plainly() {
    crate::locale::register();
    assert_eq!(
        Transfer(&info(1, DownloadState::Finished, 2_000, Some(2_000))).status_line(),
        "2\u{a0}kB"
    );
    assert_eq!(
        Transfer(&info(1, DownloadState::Failed, 0, None)).status_line(),
        "Failed"
    );
    assert_eq!(
        Transfer(&info(1, DownloadState::Cancelled, 0, None)).status_line(),
        "Cancelled"
    );
    assert!(
        Transfer(&info(1, DownloadState::Paused, 1_000, Some(2_000)))
            .status_line()
            .starts_with("Paused")
    );
}

#[test]
fn the_list_is_newest_first_and_counts_what_the_person_has_not_seen() {
    crate::locale::register();
    let mut downloads = Downloads::new();
    downloads.update(info(1, DownloadState::Running, 0, Some(100)));
    downloads.update(info(2, DownloadState::Running, 0, Some(100)));
    assert_eq!(downloads.list()[0].id, DownloadId(2));
    assert_eq!((downloads.active(), downloads.unseen()), (2, 2));
    downloads.mark_seen();
    downloads.update(info(1, DownloadState::Running, 50, Some(100)));
    assert_eq!(downloads.unseen(), 0, "progress is not news");
    downloads.update(info(1, DownloadState::Finished, 100, Some(100)));
    assert_eq!(
        (downloads.active(), downloads.unseen()),
        (1, 1),
        "finishing is"
    );
    downloads.clear_finished();
    assert_eq!(downloads.list().len(), 1);
}

#[test]
fn the_button_shows_the_share_of_everything_running() {
    crate::locale::register();
    let mut downloads = Downloads::new();
    downloads.update(info(1, DownloadState::Running, 50, Some(100)));
    downloads.update(info(2, DownloadState::Running, 0, Some(100)));
    assert_eq!(downloads.overall(), Some(0.25));
    downloads.update(info(3, DownloadState::Running, 10, None));
    assert_eq!(
        downloads.overall(),
        None,
        "one of unknown size makes the share unknown"
    );
}

fn download(id: u64, private: bool) -> DownloadInfo {
    DownloadInfo {
        id: DownloadId(id),
        url: format!("https://example.com/{id}"),
        filename: format!("file-{id}.bin"),
        path: None,
        mime: "application/octet-stream".to_string(),
        state: DownloadState::Running,
        received: 0,
        total: Some(100),
        speed: 0,
        container: 0,
        private,
    }
}

#[test]
fn downloads_are_listed_with_a_notice_and_can_be_controlled() {
    crate::locale::register();
    let (mut browser, double) = browser();
    go(&mut browser, "files.example.net");
    assert_eq!(browser.downloads.list().len(), 1);
    let notices = browser.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.text == "Downloading report-2026.pdf" && notice.action.is_some())
    );
    double.advance_downloads(10_000_000);
    browser.poll();
    let notices = browser.take_notices();
    assert!(
        notices
            .iter()
            .any(|notice| notice.text == "report-2026.pdf was downloaded")
    );
    assert_eq!(browser.downloads.active(), 0);
}

#[test]
fn a_private_download_is_not_in_a_normal_windows_list() {
    crate::locale::register();
    let (mut browser, _double) = browser();
    browser.apply(BrowserEvent::Download(download(1, true)));
    browser.apply(BrowserEvent::Download(download(2, false)));
    let ids: Vec<DownloadId> = browser.downloads.list().iter().map(|d| d.id).collect();
    assert_eq!(ids, [DownloadId(2)]);
    assert!(
        browser
            .take_notices()
            .iter()
            .all(|notice| !notice.text.contains("file-1")),
        "nor does it make a toast"
    );
    assert_eq!(browser.downloads.unseen(), 1);
}

#[test]
fn a_private_download_is_in_a_private_windows_list_and_a_normal_one_is_unchanged() {
    crate::locale::register();
    let double = TestDoubleBackend::new();
    let mut browser = Browser::new(double, Options { private: true });
    browser.apply(BrowserEvent::Download(download(1, true)));
    browser.apply(BrowserEvent::Download(download(2, false)));
    let list = browser.downloads.list();
    assert_eq!(list.len(), 2);
    assert!(list[1].private && !list[0].private, "each keeps its flag");
    assert!(
        browser
            .take_notices()
            .iter()
            .any(|notice| notice.text == "Downloading file-1.bin")
    );
}

#[test]
fn a_normal_download_is_unchanged_in_a_normal_window() {
    crate::locale::register();
    let (mut browser, _double) = browser();
    browser.apply(BrowserEvent::Download(download(5, false)));
    let list = browser.downloads.list();
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0].id, list[0].private, list[0].container),
        (DownloadId(5), false, 0)
    );
}

#[test]
fn clearing_the_list_has_the_engine_forget_what_is_over_and_nothing_else() {
    crate::locale::register();
    let (mut browser, double) = browser();
    go(&mut browser, "files.example.net");
    browser.apply(BrowserEvent::Download(download(8, false)));
    double.advance_downloads(10_000_000);
    browser.poll();
    // The page's download is over; the two the test added still go.
    let mut running = download(9, false);
    running.total = None;
    browser.apply(BrowserEvent::Download(running));
    double.clear_calls();
    let over: Vec<DownloadId> = browser
        .downloads
        .list()
        .iter()
        .filter(|d| !matches!(d.state, DownloadState::Running | DownloadState::Paused))
        .map(|d| d.id)
        .collect();
    assert!(!over.is_empty());
    browser.clear_finished_downloads();
    let removed: Vec<DownloadId> = double
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::RemoveDownload(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(removed.len(), over.len());
    for id in &over {
        assert!(removed.contains(id), "{id:?} is forgotten");
    }
    assert!(
        !removed.contains(&DownloadId(8)) && !removed.contains(&DownloadId(9)),
        "a running one is not"
    );
    let left: Vec<DownloadId> = browser.downloads.list().iter().map(|d| d.id).collect();
    assert_eq!(left, [DownloadId(9), DownloadId(8)]);
}

/// The window's controls pressed the way a person presses them, over the test
/// double: what opens the downloads list.
///
/// Failures these tests guard against:
///   1. the toolbar's downloads button does nothing while a download runs,
///      which is when the person wants it (the button was busy, and a busy
///      icon button swallows presses);
///   2. the toast's "Show" does not open the list;
///   3. the button does nothing when nothing runs either (the control);
///   4. Escape does not close the downloads list, which every popover is
///      documented to do (a page that holds the keyboard hears it first);
///   5. the downloads button's tooltip stays up behind the list that opens
///      over it, peeking out from under its edge (a popover is a modal
///      surface, and tips go down for those);
///   6. a download that failed or was cancelled has no way to be tried
///      again (the engine's Resume takes it from where it stopped).
#[cfg(feature = "snapshot")]
mod pressed {
    use bingux_ui::snapshot::Snapshot;
    use bingux_ui::web_view::BrowserEvent;
    use freya::prelude::*;

    use crate::commands::Command;
    use crate::snapshot::{self, Rig, ctx, wait};
    use crate::target::Target;

    fn open(shot: &mut Snapshot) -> bool {
        let _ = shot;
        ctx().downloads_popover.is_open()
    }

    fn window() -> (Snapshot, Rig) {
        snapshot::window(&["files.example.net"])
    }

    #[test]
    fn the_downloads_button_opens_the_list_while_a_download_runs() {
        let (mut shot, _rig) = window();
        assert_eq!(ctx().browser.peek().downloads.active(), 1);
        assert!(
            shot.click_name("Downloads: 1 running"),
            "the button is there"
        );
        wait(&mut shot, 400);
        assert!(open(&mut shot));
    }

    #[test]
    fn the_downloads_button_opens_the_list_when_nothing_runs() {
        let (mut shot, rig) = window();
        rig.double.advance_downloads(10_000_000);
        wait(&mut shot, 400);
        assert_eq!(ctx().browser.peek().downloads.active(), 0);
        assert!(shot.click_name("Downloads"), "the button is there");
        wait(&mut shot, 400);
        assert!(open(&mut shot));
    }

    #[test]
    fn the_toasts_show_opens_the_list() {
        let (mut shot, _rig) = window();
        // The button's words are drawn text, not an accessible name.
        let shown = shot.runner().find_many(|node, element| {
            let label = Label::try_downcast(element)?;
            (label.text == "Show").then(|| Area::center(&node.layout().area))
        });
        let centre = shown.first().expect("the toast for the download is up");
        shot.click(centre.x, centre.y);
        wait(&mut shot, 400);
        assert!(open(&mut shot));
    }

    #[test]
    fn escape_closes_the_downloads_list() {
        let (mut shot, _rig) = window();
        // The person clicked into the page: it holds the keyboard.
        shot.click(400.0, 400.0);
        wait(&mut shot, 200);
        let at = ctx();
        shot.runner().run_in(|| at.run(Command::Downloads));
        wait(&mut shot, 400);
        assert!(open(&mut shot), "the list is open");
        shot.run_script("key escape", Target::SCHEME)
            .expect("a key");
        wait(&mut shot, 400);
        assert!(!open(&mut shot), "Escape closed it");
    }

    /// The drawn bubbles that say `text`: the visible one and its hidden
    /// measuring copy are both labels, and a tip that is down has neither.
    fn bubbles(shot: &mut Snapshot, text: &str) -> usize {
        shot.runner()
            .find_many(|_, element| {
                let label = Label::try_downcast(element)?;
                (label.text == text).then_some(())
            })
            .len()
    }

    #[test]
    fn a_tooltip_goes_down_when_the_list_opens_over_its_button() {
        let (mut shot, _rig) = window();
        let tip = "Downloads: 1 running";
        let button = shot.runner().find_many(|node, element| {
            (element.accessibility().builder.label() == Some(tip))
                .then(|| Area::center(&node.layout().area))
        });
        let at = button.first().expect("the button is there");
        shot.runner()
            .move_cursor((f64::from(at.x), f64::from(at.y)));
        wait(&mut shot, 900);
        assert!(
            bubbles(&mut shot, tip) > 0,
            "the tip is up while the pointer rests"
        );
        let window = ctx();
        shot.runner().run_in(|| window.run(Command::Downloads));
        wait(&mut shot, 900);
        assert!(open(&mut shot), "the list is open");
        assert_eq!(bubbles(&mut shot, tip), 0, "the tip went down with it");
    }

    /// A download the engine says failed: the page's own (id 1), as it stood.
    fn fail_the_download(rig: &Rig) {
        let mut failed = ctx().browser.peek().downloads.list()[0].clone();
        failed.state = bingux_ui::web_view::requests::DownloadState::Failed;
        failed.speed = 0;
        rig.double.emit(BrowserEvent::Download(failed));
    }

    #[test]
    fn a_failed_download_can_be_tried_again() {
        use bingux_ui::web_view::requests::DownloadState;
        use bingux_ui::web_view::test_double::Call;
        let (mut shot, rig) = window();
        let id = ctx().browser.peek().downloads.list()[0].id;
        fail_the_download(&rig);
        wait(&mut shot, 300);
        assert_eq!(
            ctx().browser.peek().downloads.list()[0].state,
            DownloadState::Failed
        );
        let at = ctx();
        shot.runner().run_in(|| at.run(Command::Downloads));
        wait(&mut shot, 400);
        assert!(
            shot.click_name("Retry"),
            "a failed download has a Retry button"
        );
        wait(&mut shot, 400);
        assert!(
            rig.double
                .calls()
                .iter()
                .any(|call| matches!(call, Call::ResumeDownload(known) if *known == id)),
            "the engine was asked to resume it"
        );
        assert_eq!(
            ctx().browser.peek().downloads.list()[0].state,
            DownloadState::Running
        );
    }

    #[test]
    fn a_download_that_runs_has_no_retry() {
        let (mut shot, _rig) = window();
        let at = ctx();
        shot.runner().run_in(|| at.run(Command::Downloads));
        wait(&mut shot, 400);
        assert!(!shot.click_name("Retry"), "pause and cancel, not retry");
    }
}
