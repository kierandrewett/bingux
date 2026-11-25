//! Events from the engine thread to the UI thread, and the wake-up between
//! them.
//!
//! [`EventQueue`] is what a backend keeps: the engine's thread pushes, the
//! UI thread drains, and the waker the UI gave is called when the queue goes
//! from empty to not empty. [`Signal`] is the UI's side: a future that
//! resolves once for every wake, so a task `await`s it between drains and
//! nothing polls on a timer.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Wake, Waker};

use crate::events::BrowserEvent;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The events waiting for the UI, and the waker to call for them.
#[derive(Default)]
pub struct EventQueue {
    events: Mutex<VecDeque<BrowserEvent>>,
    waker: Mutex<Option<Waker>>,
}

impl EventQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue an event. Wakes the UI when the queue was empty.
    pub fn push(&self, event: BrowserEvent) {
        let was_empty = {
            let mut events = lock(&self.events);
            let was_empty = events.is_empty();
            events.push_back(event);
            was_empty
        };
        if was_empty {
            self.wake();
        }
    }

    /// Queue several events with one wake.
    pub fn extend(&self, events: impl IntoIterator<Item = BrowserEvent>) {
        let was_empty = {
            let mut queue = lock(&self.events);
            let was_empty = queue.is_empty();
            let before = queue.len();
            queue.extend(events);
            was_empty && queue.len() > before
        };
        if was_empty {
            self.wake();
        }
    }

    pub fn set_waker(&self, waker: Waker) {
        let pending = !lock(&self.events).is_empty();
        *lock(&self.waker) = Some(waker);
        // Events that arrived before anybody listened are not lost.
        if pending {
            self.wake();
        }
    }

    /// Moves every queued event into `out`, in order.
    pub fn drain_into(&self, out: &mut Vec<BrowserEvent>) {
        out.extend(lock(&self.events).drain(..));
    }

    fn wake(&self) {
        if let Some(waker) = lock(&self.waker).as_ref() {
            waker.wake_by_ref();
        }
    }
}

/// The UI's side of the wake-up.
#[derive(Default)]
pub struct Signal {
    pending: AtomicBool,
    task: Mutex<Option<Waker>>,
}

impl Signal {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// The waker to give [`EventQueue::set_waker`]. A wake that comes before
    /// the task looks still counts.
    pub fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }

    /// Resolves once there has been a wake since it last resolved.
    pub fn notified(self: &Arc<Self>) -> Notified {
        Notified { signal: self.clone() }
    }
}

impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.pending.store(true, Ordering::Release);
        if let Some(task) = lock(&self.task).as_ref() {
            task.wake_by_ref();
        }
    }
}

/// See [`Signal::notified`].
pub struct Notified {
    signal: Arc<Signal>,
}

impl Future for Notified {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let signal = &self.signal;
        if signal.pending.swap(false, Ordering::AcqRel) {
            return Poll::Ready(());
        }
        {
            let mut task = lock(&signal.task);
            match task.as_mut() {
                Some(stored) => stored.clone_from(cx.waker()),
                None => *task = Some(cx.waker().clone()),
            }
        }
        // A wake between the check and the store found no task to wake.
        if signal.pending.swap(false, Ordering::AcqRel) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;
    use crate::events::EngineEvent;
    use crate::ids::TabId;
    use crate::events::TabEvent;

    struct Counter(AtomicUsize);

    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn the_waker_is_called_when_the_queue_goes_from_empty_to_not_empty_only() {
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        let queue = EventQueue::new();
        queue.set_waker(Waker::from(counter.clone()));
        queue.push(BrowserEvent::Engine(EngineEvent::Ready));
        queue.push(BrowserEvent::tab(TabId(1), TabEvent::Ready));
        assert_eq!(counter.0.load(Ordering::SeqCst), 1, "one wake for two events");
        let mut out = Vec::new();
        queue.drain_into(&mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], BrowserEvent::Engine(EngineEvent::Ready), "in order");
        queue.push(BrowserEvent::tab(TabId(1), TabEvent::Title("x".into())));
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn events_from_before_the_listener_are_announced_when_it_arrives() {
        let queue = EventQueue::new();
        queue.push(BrowserEvent::Engine(EngineEvent::Ready));
        let counter = Arc::new(Counter(AtomicUsize::new(0)));
        queue.set_waker(Waker::from(counter.clone()));
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_wake_before_the_task_looks_still_resolves_it() {
        let signal = Signal::new();
        signal.waker().wake();
        let mut future = Box::pin(signal.notified());
        let waker = Waker::from(Arc::new(Counter(AtomicUsize::new(0))));
        let mut cx = Context::from_waker(&waker);
        assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(()));
        assert_eq!(future.as_mut().poll(&mut cx), Poll::Pending, "once per wake");
    }
}
