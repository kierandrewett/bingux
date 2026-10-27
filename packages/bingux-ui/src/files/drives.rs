//! The drives of the places list: the mount table read at once, sizes read
//! in the background, and the list kept current as drives come and go.
//!
//! Nothing here touches a mount on the UI thread. The table comes from
//! `/proc/self/mountinfo`, which never blocks on a share. The sizes
//! (`statvfs`) are asked for on a worker with a deadline, so a share that
//! has gone away shows as "Not responding" instead of freezing the window.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use bingux_files::mounts::{Mount, fill_space, list_mounts, watch_mounts};
use freya::prelude::*;

use super::places_model::{Release, carry_usage};
use crate::shell::Overlays;
use crate::task::spawn_blocking;

#[cfg(any(test, feature = "testing"))]
#[path = "places_fixture.rs"]
pub(crate) mod fixture;

/// How long a drive may take to say how full it is.
const SPACE_DEADLINE: Duration = Duration::from_secs(2);

/// The drives and whether their sizes have been asked for yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DriveList {
    pub mounts: Vec<Mount>,
    /// False until the first read of the sizes finishes, and again while the
    /// table changed and its new drives are being sized.
    pub settled: bool,
}

/// The drives of this window and the ones being let go of. Copy it into
/// handlers.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Drives {
    list: State<DriveList>,
    busy: State<Vec<u32>>,
    overlays: Option<Overlays>,
}

/// The drives, read now and kept current. Call it once per component, from
/// `render`. The component that reads [`Drives::list`] redraws when a drive
/// is added, removed or sized.
pub(crate) fn use_drives() -> Drives {
    let list = use_state(initial);
    let busy = use_state(Vec::new);
    let overlays = try_consume_context::<Overlays>();
    use_hook(|| Rc::new(spawn_refresh(list)));
    Drives {
        list,
        busy,
        overlays,
    }
}

fn initial() -> DriveList {
    #[cfg(any(test, feature = "testing"))]
    if let Some(mounts) = fixture::mounts() {
        return DriveList {
            mounts: mounts.clone(),
            settled: true,
        };
    }
    DriveList {
        mounts: list_mounts(),
        settled: false,
    }
}

fn faked() -> bool {
    #[cfg(any(test, feature = "testing"))]
    {
        fixture::mounts().is_some()
    }
    #[cfg(not(any(test, feature = "testing")))]
    {
        false
    }
}

impl Drives {
    /// The drives now. Reading it subscribes the calling component.
    pub fn list(&self) -> DriveList {
        self.list.read().clone()
    }

    /// Whether the drive with this mount id is being let go of.
    pub fn is_busy(&self, id: u32) -> bool {
        self.busy.read().contains(&id)
    }

    /// Unmount or eject `mount` on a worker. Everything the kernel still holds
    /// for the drive is written out first (see
    /// [`bingux_files::mounts::unmount`]), so a drive that cannot take the
    /// data is not detached. The row shows a busy state until it is done; a
    /// failure is said in a toast, in the words of the tool that failed
    /// (a busy drive says so). Ejecting says when the drive can be taken out,
    /// which is the only time it is safe to.
    pub fn release(self, mount: Mount, how: Release) {
        let mut busy = self.busy;
        if busy.peek().contains(&mount.id) {
            return;
        }
        busy.write().push(mount.id);
        let overlays = self.overlays;
        spawn(async move {
            let id = mount.id;
            let label = mount.label.clone();
            let outcome = spawn_blocking(move || run_release(&mount, how)).await;
            busy.write().retain(|busy_id| *busy_id != id);
            match (outcome, overlays) {
                (Err(sentence), Some(overlays)) => overlays.toast(sentence),
                (Ok(()), Some(overlays)) if how == Release::Eject => {
                    overlays.toast(format!("{label} is safe to remove."));
                }
                _ => {}
            }
        });
    }
}

fn run_release(mount: &Mount, how: Release) -> Result<(), String> {
    if faked() {
        std::thread::sleep(Duration::from_secs(6));
        return Err(format!(
            "{} is busy. Close the files on it and try again.",
            mount.label
        ));
    }
    match how {
        Release::Eject => bingux_files::mounts::eject(mount),
        Release::Unmount => bingux_files::mounts::unmount(mount),
    }
}

/// The task that keeps `list` current: size the drives, then wait for the
/// mount table to change. A changed table shows at once, with the sizes
/// already known carried over, and is sized again.
fn spawn_refresh(mut list: State<DriveList>) -> Option<TaskHandle> {
    if faked() {
        return None;
    }
    Some(spawn(async move {
        let wake = Arc::new(Wake::default());
        let notifier = wake.clone();
        // Held for the life of the task; dropping it stops the thread.
        let _watch = watch_mounts(move || notifier.notify());
        loop {
            let mounts = spawn_blocking(|| {
                let mut mounts = list_mounts();
                fill_space(&mut mounts, SPACE_DEADLINE);
                mounts
            })
            .await;
            list.set(DriveList {
                mounts,
                settled: true,
            });
            wake.wait().await;
            let table = carry_usage(list_mounts(), &list.peek().mounts);
            list.set(DriveList {
                mounts: table,
                settled: false,
            });
        }
    }))
}

/// A wake-up from the thread that watches the mount table to the task that
/// reads it again. A call that arrives while nobody waits is kept.
#[derive(Default)]
struct Wake {
    state: Mutex<(bool, Option<Waker>)>,
}

impl Wake {
    fn notify(&self) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 = true;
        if let Some(waker) = state.1.take() {
            waker.wake();
        }
    }

    fn wait(self: &Arc<Self>) -> impl Future<Output = ()> + use<> {
        Waiting(self.clone())
    }
}

struct Waiting(Arc<Wake>);

impl Future for Waiting {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let mut state = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.0 {
            state.0 = false;
            Poll::Ready(())
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
