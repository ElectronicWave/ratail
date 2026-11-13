use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, RwLock};

use crate::redraw::Redraw;
use crate::task::{Snapshot, Task};

/// A keyed cache of [`Task`] values that share one [`Redraw`].
///
/// This is closer to a keyed query cache than to React's `useEffect` dependency
/// list: every key keeps its own result until it is removed or cleared. Use it
/// when a service serves several parameter combinations and those results are
/// worth retaining, such as a catalog per driver.
///
/// Keys can be tuples or structs when a request depends on several parameters.
pub struct TaskCache<K, T, E> {
    redraw: Redraw,
    tasks: RwLock<HashMap<K, Task<T, E>>>,
}

impl<K, T, E> TaskCache<K, T, E>
where
    K: Eq + Hash + Clone,
{
    /// Creates an empty cache whose tasks request a repaint through `redraw`.
    pub fn new(redraw: &Redraw) -> Self {
        Self {
            redraw: redraw.clone(),
            tasks: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the cached task for `key` without creating an entry.
    pub fn get(&self, key: &K) -> Option<Task<T, E>> {
        self.tasks
            .read()
            .expect("task cache lock poisoned")
            .get(key)
            .cloned()
    }

    /// Returns the task for `key`, creating an idle task when the key is new.
    pub fn get_or_create(&self, key: &K) -> Task<T, E> {
        if let Some(task) = self.get(key) {
            return task;
        }

        self.tasks
            .write()
            .expect("task cache lock poisoned")
            .entry(key.clone())
            .or_insert_with(|| Task::with_redraw(&self.redraw))
            .clone()
    }

    /// Returns the current snapshot only when `key` has already been requested.
    pub fn snapshot(&self, key: &K) -> Option<Arc<Snapshot<T, E>>> {
        self.get(key).map(|task| task.snapshot())
    }

    /// Drops the entry for `key` and cancels its in-flight request, if any.
    pub fn remove(&self, key: &K) -> Option<Task<T, E>> {
        let task = self
            .tasks
            .write()
            .expect("task cache lock poisoned")
            .remove(key)?;
        task.cancel();
        Some(task)
    }

    /// Drops every entry and cancels all in-flight requests.
    pub fn clear(&self) {
        let tasks = std::mem::take(&mut *self.tasks.write().expect("task cache lock poisoned"));

        for task in tasks.values() {
            task.cancel();
        }
    }

    /// Starts a request for `key` when that key is not already running.
    ///
    /// `make` receives the key so each request can capture its own parameters.
    ///
    /// Returns `true` when a new future was spawned.
    pub fn spawn_once<F, Fut>(&self, key: K, make: F) -> bool
    where
        K: Send + 'static,
        T: Send + Sync + 'static,
        E: Send + Sync + 'static,
        F: FnOnce(K) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
    {
        let task = self.get_or_create(&key);
        task.spawn_once(move || make(key))
    }
}
