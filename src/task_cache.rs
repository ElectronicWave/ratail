use std::future::Future;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

use lru::LruCache;

use crate::redraw::Redraw;
use crate::task::{Snapshot, Task};

const DEFAULT_CAPACITY: usize = 32;

/// A keyed LRU cache of [`Task`] values that share one [`Redraw`].
///
/// This is closer to a keyed query cache than to React's `useEffect` dependency
/// list: every key keeps its own result, so results for different parameters
/// never overwrite each other. The cache holds at most 32 entries by default.
/// Inserting a new key evicts the least recently used entry and cancels its
/// in-flight request. Use [`TaskCache::with_capacity`] to choose another bound.
///
/// Keys can be tuples or structs when a request depends on several parameters.
pub struct TaskCache<K, T, E> {
    redraw: Redraw,
    tasks: RwLock<LruCache<K, Task<T, E>>>,
}

impl<K, T, E> TaskCache<K, T, E>
where
    K: Eq + Hash + Clone,
{
    /// Creates an empty cache with the default capacity of 32 entries.
    ///
    /// Tasks request a repaint through `redraw`.
    pub fn new(redraw: &Redraw) -> Self {
        let capacity =
            NonZeroUsize::new(DEFAULT_CAPACITY).expect("default cache capacity is non-zero");
        Self::with_capacity(redraw, capacity)
    }

    /// Creates an empty cache that holds at most `capacity` entries.
    ///
    /// Tasks request a repaint through `redraw`.
    pub fn with_capacity(redraw: &Redraw, capacity: NonZeroUsize) -> Self {
        Self {
            redraw: redraw.clone(),
            tasks: RwLock::new(LruCache::new(capacity)),
        }
    }

    /// Returns the cached task for `key` and marks it as recently used.
    ///
    /// Returns `None` without creating an entry when `key` is absent.
    pub fn get(&self, key: &K) -> Option<Task<T, E>> {
        self.tasks
            .write()
            .expect("task cache lock poisoned")
            .get(key)
            .cloned()
    }

    /// Returns the task for `key`, creating an idle task when the key is new.
    ///
    /// The key is marked as recently used. When the cache is full, the least
    /// recently used entry is removed and its in-flight request is cancelled.
    pub fn get_or_create(&self, key: &K) -> Task<T, E> {
        let mut tasks = self.tasks.write().expect("task cache lock poisoned");
        self.get_or_insert(&mut tasks, key)
    }

    /// Returns the current snapshot only when `key` has already been requested.
    ///
    /// This is a cheap read: it does not create an entry and does not change
    /// LRU recency. Use [`TaskCache::get`] when reading should count as use.
    pub fn snapshot(&self, key: &K) -> Option<Arc<Snapshot<T, E>>> {
        let task = self
            .tasks
            .read()
            .expect("task cache lock poisoned")
            .peek(key)
            .cloned()?;
        Some(task.snapshot())
    }

    /// Drops the entry for `key` and cancels its in-flight request, if any.
    pub fn remove(&self, key: &K) -> Option<Task<T, E>> {
        let task = self
            .tasks
            .write()
            .expect("task cache lock poisoned")
            .pop(key)?;
        task.cancel();
        Some(task)
    }

    /// Drops every entry and cancels all in-flight requests.
    pub fn clear(&self) {
        let mut tasks = self.tasks.write().expect("task cache lock poisoned");
        while let Some((_key, task)) = tasks.pop_lru() {
            task.cancel();
        }
    }

    /// Starts a request for `key` when that key is not already running.
    ///
    /// `make` receives the key so each request can capture its own parameters.
    ///
    /// The key is marked as recently used. When the cache is full, the least
    /// recently used entry is removed and its in-flight request is cancelled.
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
        let mut tasks = self.tasks.write().expect("task cache lock poisoned");
        let task = self.get_or_insert(&mut tasks, &key);
        task.spawn_once(move || make(key))
    }

    fn get_or_insert(&self, tasks: &mut LruCache<K, Task<T, E>>, key: &K) -> Task<T, E> {
        if let Some(task) = tasks.get(key) {
            return task.clone();
        }

        let task = Task::with_redraw(&self.redraw);
        if let Some((_evicted_key, evicted_task)) = tasks.push(key.clone(), task.clone()) {
            evicted_task.cancel();
        }
        task
    }
}
