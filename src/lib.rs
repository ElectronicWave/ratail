//! A small ratatui application kernel.
//!
//! Layering:
//! - [`Context`] holds typed application services shared by every page.
//! - [`Task`] is a cloneable observable future. Store it in a service when
//!   multiple pages need the same result. Give it a [`Redraw`] signal so the
//!   application repaints when the snapshot changes.
//! - [`Page`] owns route-local state and behavior. Its route type defaults to
//!   [`Context::Route`], but a shell page can embed an [`Navigator`] for a
//!   section-local route type. After a [`Redraw`] request, the active page's
//!   `poll` hook runs before the next frame so pages can react to completed
//!   tasks without doing work in `render`.
//! - [`Navigator`] owns a route stack and page lifecycle.
//! - [`ModalHost`] owns a callback-driven modal stack for prompts, pickers, and
//!   confirmations.
//! - [`App`] drives rendering and input routing on the current tokio runtime.
//!
//! [`Task`] and [`App::run`] require a tokio runtime; pages themselves are
//! synchronous.

pub mod app;
pub mod context;
pub mod input;
pub mod modal;
pub mod navigator;
pub mod page;
pub mod redraw;
pub mod task;
pub mod task_cache;

pub use app::{App, AppFlow, hint_line};
pub use context::Context;
pub use input::{
    FocusRing, Hit, HitMap, InputEvent, KeyChord, KeyHint, KeyMap, MouseInput, MouseKind,
};
pub use modal::{IntoPageEvent, Modal, ModalHost, ModalOutcome, ModalPage, WithModal};
pub use navigator::{NavAction, Navigator, Router};
pub use page::{Page, PageEvent};
pub use redraw::Redraw;
pub use task::{Snapshot, SnapshotStatus, SnapshotView, Task};
pub use task_cache::TaskCache;
