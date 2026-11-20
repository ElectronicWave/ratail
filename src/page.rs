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

    /// Hints for the current page. This is called on every frame, so pages can
    /// regenerate localized hints when the active locale changes.
    fn keybindings(&self, _route: &R) -> Vec<KeyHint> {
        Vec::new()
    }

    fn render(&mut self, ctx: &C, route: &R, frame: &mut Frame, area: Rect);
}
