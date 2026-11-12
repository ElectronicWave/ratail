use std::sync::Arc;

use tokio::sync::Notify;

/// A cloneable repaint signal shared by services, tasks, and the application loop.
#[derive(Clone, Debug, Default)]
pub struct Redraw {
    notify: Arc<Notify>,
}

impl Redraw {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests one repaint. Multiple requests before the next frame collapse.
    pub fn trigger(&self) {
        self.notify.notify_one();
    }

    /// Resolves when a repaint has been requested.
    pub async fn requested(&self) {
        self.notify.notified().await;
    }
}
