use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use tokio::sync::Notify;

use crate::redraw::Redraw;

/// Observable state of an async task.
#[derive(Debug)]
pub enum Snapshot<T, E> {
    Idle,
    Loading {
        previous: Option<Arc<T>>,
    },
    Ready(Arc<T>),
    Error {
        error: Arc<E>,
        previous: Option<Arc<T>>,
    },
    Cancelled {
        previous: Option<Arc<T>>,
    },
}

impl<T, E> Snapshot<T, E> {
    pub fn value(&self) -> Option<&Arc<T>> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }

    pub fn previous(&self) -> Option<&Arc<T>> {
        match self {
            Self::Loading { previous }
            | Self::Error { previous, .. }
            | Self::Cancelled { previous } => previous.as_ref(),
            Self::Idle | Self::Ready(_) => None,
        }
    }

    pub fn error(&self) -> Option<&Arc<E>> {
        match self {
            Self::Error { error, .. } => Some(error),
            _ => None,
        }
    }
}

struct TaskInner<T, E> {
    snapshot: ArcSwap<Snapshot<T, E>>,
    running: AtomicBool,
    run: Mutex<Option<Arc<Run>>>,
}

#[derive(Default)]
struct Run {
    cancelled: AtomicBool,
    notify: Notify,
}

/// A shared, observable async task.
pub struct Task<T, E> {
    inner: Arc<TaskInner<T, E>>,
    redraw: Option<Redraw>,
}

impl<T, E> Clone for Task<T, E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            redraw: self.redraw.clone(),
        }
    }
}

impl<T, E> Default for Task<T, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, E> Task<T, E> {
    pub fn new() -> Self {
        Self::from_redraw(None)
    }

    /// Creates a task that requests a repaint whenever its snapshot changes.
    pub fn with_redraw(redraw: &Redraw) -> Self {
        Self::from_redraw(Some(redraw.clone()))
    }

    fn from_redraw(redraw: Option<Redraw>) -> Self {
        Self {
            inner: Arc::new(TaskInner {
                snapshot: ArcSwap::new(Arc::new(Snapshot::Idle)),
                running: AtomicBool::new(false),
                run: Mutex::new(None),
            }),
            redraw,
        }
    }

    fn wake(&self) {
        if let Some(redraw) = &self.redraw {
            redraw.trigger();
        }
    }

    pub fn snapshot(&self) -> Arc<Snapshot<T, E>> {
        self.inner.snapshot.load_full()
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::Acquire)
    }

    /// Cancels the currently running future and records a cancelled snapshot.
    ///
    /// Does nothing when no future is running.
    pub fn cancel(&self) {
        let run = self.inner.run.lock().expect("task run lock poisoned");
        let Some(run) = run.as_ref() else {
            return;
        };

        run.cancelled.store(true, Ordering::Release);
        run.notify.notify_one();

        let previous = self.previous();
        self.inner
            .snapshot
            .store(Arc::new(Snapshot::Cancelled { previous }));
        self.wake();
    }

    /// Resets the task to `Idle` if it is not running.
    pub fn reset(&self) -> bool {
        let _run = self.inner.run.lock().expect("task run lock poisoned");
        if self.is_running() {
            return false;
        }

        self.inner.snapshot.store(Arc::new(Snapshot::Idle));
        self.wake();
        true
    }

    fn previous(&self) -> Option<Arc<T>> {
        match &*self.snapshot() {
            Snapshot::Ready(value) => Some(value.clone()),
            Snapshot::Loading { previous } | Snapshot::Error { previous, .. } => previous.clone(),
            Snapshot::Cancelled { previous } => previous.clone(),
            Snapshot::Idle => None,
        }
    }
}

impl<T, E> Task<T, E>
where
    T: Send + Sync + 'static,
    E: Send + Sync + 'static,
{
    /// Starts the future if the task is not already running.
    ///
    /// The future is spawned on the current tokio runtime, so this must be
    /// called from inside one.
    ///
    /// Returns `true` when a new future was spawned.
    pub fn spawn_once<F, Fut>(&self, make: F) -> bool
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let mut slot = self.inner.run.lock().expect("task run lock poisoned");
        if self.inner.running.swap(true, Ordering::AcqRel) {
            return false;
        }

        let previous = self.previous();
        self.inner.snapshot.store(Arc::new(Snapshot::Loading {
            previous: previous.clone(),
        }));
        self.wake();

        let run = Arc::new(Run::default());
        *slot = Some(run.clone());
        drop(slot);

        let inner = self.inner.clone();
        let redraw = self.redraw.clone();
        tokio::spawn(async move {
            let _guard = RunningGuard {
                inner: inner.clone(),
                run: run.clone(),
            };

            let result = tokio::select! {
                biased;
                _ = run.notify.notified() => {
                    return;
                }
                result = make() => result,
            };

            let snapshot = match result {
                Ok(value) => Snapshot::Ready(Arc::new(value)),
                Err(error) => Snapshot::Error {
                    error: Arc::new(error),
                    previous,
                },
            };

            let _run = inner.run.lock().expect("task run lock poisoned");
            if run.cancelled.load(Ordering::Acquire) {
                return;
            }

            inner.snapshot.store(Arc::new(snapshot));

            if let Some(redraw) = redraw {
                redraw.trigger();
            }
        });

        true
    }
}

struct RunningGuard<T, E> {
    inner: Arc<TaskInner<T, E>>,
    run: Arc<Run>,
}

impl<T, E> Drop for RunningGuard<T, E> {
    fn drop(&mut self) {
        let mut current = self.inner.run.lock().expect("task run lock poisoned");
        if current
            .as_ref()
            .is_some_and(|run| Arc::ptr_eq(run, &self.run))
        {
            *current = None;
        }
        drop(current);

        self.inner.running.store(false, Ordering::Release);
    }
}
