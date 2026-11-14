//! A small ratatui application kernel.
//!
//! Layering:
//! - [`Context`] holds typed application services shared by every page.
//! - [`Task`] is a cloneable observable future. Store it in a service when
//!   multiple pages need the same result. Give it a [`Redraw`] signal so the
//!   application repaints when the snapshot changes.
//! - [`Page`] owns route-local state and behavior.
//! - [`Navigator`] owns the route stack and page lifecycle.
//! - [`App`] drives rendering and key routing on the current tokio runtime.
//!
//! [`Task`] and [`App::run`] require a tokio runtime; pages themselves are
//! synchronous.

pub mod app;
pub mod context;
pub mod input;
pub mod navigator;
pub mod page;
pub mod redraw;
pub mod task;
pub mod task_cache;

pub use app::{App, AppFlow, hint_line};
pub use context::Context;
pub use input::{FocusRing, KeyChord, KeyHint, KeyMap};
pub use navigator::{NavAction, Navigator, Router};
pub use page::{Page, PageEvent};
pub use redraw::Redraw;
pub use task::{Snapshot, SnapshotStatus, SnapshotView, Task};
pub use task_cache::TaskCache;
