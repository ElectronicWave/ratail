use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::context::Context;
use crate::input::KeyHint;
use crate::navigator::NavAction;

/// What a page wants the application to do after handling a key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageEvent<R> {
    Ignored,
    Handled,
    Navigate(NavAction<R>),
}

/// A route's page state and behavior.
///
/// `R` defaults to [`Context::Route`], so a top-level page only needs
/// `impl Page<C>`. Embedded pages can use a section-local route type, such as
/// `impl Page<C, AccountRoute>`.
pub trait Page<C: Context, R = <C as Context>::Route> {
    fn init(&mut self, _ctx: &C, _route: &R) {}

    fn resume(&mut self, _ctx: &C, _route: &R) {}

    fn pause(&mut self, _ctx: &C, _route: &R) {}

    fn dispose(&mut self, _ctx: &C, _route: &R) {}

    fn on_key(&mut self, _ctx: &C, _route: &R, _key: KeyEvent) -> PageEvent<R> {
        PageEvent::Ignored
    }

    /// Runs after a redraw is requested and before the next frame.
    ///
    /// This is the place to react to external state changes such as a
    /// completed [`Task`](crate::Task). The hook is not called for key-driven
    /// changes and is not a frame tick; it runs only when
    /// [`Redraw`](crate::Redraw) reports that something may have changed.
    ///
    /// Polling must be cheap and idempotent. Return a navigation action when
    /// the page should move; the navigator applies at most one action per
    /// redraw. A newly pushed page initializes and resumes after that action,
    /// but it is not polled again until the next redraw. Do not request a
    /// redraw unconditionally; start work that will request one when it
    /// completes.
    fn poll(&mut self, _ctx: &C, _route: &R) -> Option<NavAction<R>> {
        None
    }

    /// Hints for the current page. This is called on every frame, so pages can
    /// regenerate localized hints when the active locale changes.
    fn keybindings(&self, _route: &R) -> Vec<KeyHint> {
        Vec::new()
    }

    fn render(&mut self, ctx: &C, route: &R, frame: &mut Frame, area: Rect);
}
