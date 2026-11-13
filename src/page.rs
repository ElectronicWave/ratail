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
pub trait Page<C: Context> {
    fn init(&mut self, _ctx: &C) {}

    fn resume(&mut self, _ctx: &C) {}

    fn pause(&mut self, _ctx: &C) {}

    fn dispose(&mut self, _ctx: &C) {}

    fn on_key(&mut self, _ctx: &C, _key: KeyEvent) -> PageEvent<C::Route> {
        PageEvent::Ignored
    }

    /// Hints for the current page. This is called on every frame, so pages can
    /// regenerate localized hints when the active locale changes.
    fn keybindings(&self) -> Vec<KeyHint> {
        Vec::new()
    }

    fn render(&mut self, ctx: &C, frame: &mut Frame, area: Rect);
}
