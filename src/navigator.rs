use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::context::Context;
use crate::input::KeyHint;
use crate::page::{Page, PageEvent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NavAction<R> {
    Push(R),
    Pop,
    Replace(R),
    Reset(R),
}

/// Creates page state for a route.
///
/// `R` defaults to [`Context::Route`]. A shell page can host a nested
/// [`Navigator`] with a section-local router such as
/// `Router<C, AccountRoute>`.
pub trait Router<C: Context, R = <C as Context>::Route> {
    fn create(&mut self, route: &R, ctx: &C) -> Box<dyn Page<C, R>>;
}

/// Owns a route stack and the pages created for it.
///
/// `R` is the route type this navigator manages and `Rt` is its router. Use a
/// nested navigator with a section-local route type to keep each area's
/// navigation independent from [`Context::Route`].
pub struct Navigator<C: Context, R, Rt: Router<C, R>> {
    router: Rt,
    stack: Vec<RouteEntry<C, R>>,
}

struct RouteEntry<C: Context, R> {
    route: R,
    page: Box<dyn Page<C, R>>,
}

impl<C: Context, R, Rt: Router<C, R>> Navigator<C, R, Rt> {
    pub fn new(mut router: Rt, root: R, ctx: &C) -> Self {
        let mut page = router.create(&root, ctx);
        page.init(ctx, &root);
        page.resume(ctx, &root);

        Self {
            router,
            stack: vec![RouteEntry { route: root, page }],
        }
    }

    /// Applies a navigation action.
    ///
    /// Returns `false` when the action did not change the stack, such as a
    /// [`NavAction::Pop`] on the root page.
    pub fn apply(&mut self, action: NavAction<R>, ctx: &C) -> bool {
        match action {
            NavAction::Push(route) => {
                self.push(route, ctx);
                true
            }
            NavAction::Pop => self.pop(ctx),
            NavAction::Replace(route) => {
                self.replace(route, ctx);
                true
            }
            NavAction::Reset(route) => {
                self.reset(route, ctx);
                true
            }
        }
    }

    pub fn push(&mut self, route: R, ctx: &C) {
        if let Some(current) = self.stack.last_mut() {
            current.page.pause(ctx, &current.route);
        }

        let mut page = self.router.create(&route, ctx);
        page.init(ctx, &route);
        page.resume(ctx, &route);

        self.stack.push(RouteEntry { route, page });
    }

    /// Pops the current route unless it is the root.
    ///
    /// Returns `true` when a route was removed.
    pub fn pop(&mut self, ctx: &C) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }

        if let Some(mut entry) = self.stack.pop() {
            entry.page.pause(ctx, &entry.route);
            entry.page.dispose(ctx, &entry.route);
        }

        if let Some(current) = self.stack.last_mut() {
            current.page.resume(ctx, &current.route);
        }

        true
    }

    pub fn replace(&mut self, route: R, ctx: &C) {
        if let Some(mut current) = self.stack.pop() {
            current.page.pause(ctx, &current.route);
            current.page.dispose(ctx, &current.route);
        }

        let mut page = self.router.create(&route, ctx);
        page.init(ctx, &route);
        page.resume(ctx, &route);

        self.stack.push(RouteEntry { route, page });
    }

    pub fn reset(&mut self, route: R, ctx: &C) {
        while let Some(mut entry) = self.stack.pop() {
            entry.page.pause(ctx, &entry.route);
            entry.page.dispose(ctx, &entry.route);
        }

        let mut page = self.router.create(&route, ctx);
        page.init(ctx, &route);
        page.resume(ctx, &route);

        self.stack.push(RouteEntry { route, page });
    }

    /// Borrows the current route together with its page.
    pub fn current(&self) -> Option<(&R, &(dyn Page<C, R> + 'static))> {
        self.stack
            .last()
            .map(|entry| (&entry.route, entry.page.as_ref()))
    }

    /// Mutably borrows the current route together with its page.
    pub fn current_mut(&mut self) -> Option<(&R, &mut (dyn Page<C, R> + 'static))> {
        self.stack
            .last_mut()
            .map(|entry| (&entry.route, entry.page.as_mut()))
    }

    pub fn current_route(&self) -> Option<&R> {
        self.stack.last().map(|entry| &entry.route)
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Forwards a key to the current page and applies any resulting
    /// [`NavAction`] to this navigator.
    ///
    /// Returns `true` when the current page handled the key or navigated, and
    /// `false` when it returned [`PageEvent::Ignored`] (allowing an outer shell
    /// page to handle keys like `Esc` when the sub-navigator is at its root).
    pub fn handle_key(&mut self, ctx: &C, key: KeyEvent) -> bool {
        let event = match self.current_mut() {
            Some((route, page)) => page.on_key(ctx, route, key),
            None => PageEvent::Ignored,
        };

        match event {
            PageEvent::Ignored => false,
            PageEvent::Handled => true,
            PageEvent::Navigate(action) => {
                self.apply(action, ctx);
                true
            }
        }
    }

    /// Polls the current page once and applies at most one navigation action.
    ///
    /// Only the top page is polled. A paused page observes completed work when
    /// it is resumed or when it becomes the top page again.
    ///
    /// A page pushed by the navigation action is initialized and resumed before
    /// this method returns, but it is not polled again until the next redraw.
    ///
    /// Returns `true` when the navigation action changed the stack.
    pub fn poll_current(&mut self, ctx: &C) -> bool {
        let action = match self.current_mut() {
            Some((route, page)) => page.poll(ctx, route),
            None => None,
        };
        let Some(action) = action else {
            return false;
        };

        self.apply(action, ctx)
    }

    /// Renders the current page into `area`.
    pub fn render(&mut self, ctx: &C, frame: &mut Frame, area: Rect) {
        if let Some((route, page)) = self.current_mut() {
            page.render(ctx, route, frame, area);
        }
    }

    /// Returns the current page's key hints.
    pub fn keybindings(&self) -> Vec<KeyHint> {
        match self.current() {
            Some((route, page)) => page.keybindings(route),
            None => Vec::new(),
        }
    }

    /// Resumes the current page. Call this from an outer shell page's
    /// [`Page::resume`].
    pub fn resume_current(&mut self, ctx: &C) {
        if let Some((route, page)) = self.current_mut() {
            page.resume(ctx, route);
        }
    }

    /// Pauses the current page. Call this from an outer shell page's
    /// [`Page::pause`].
    pub fn pause_current(&mut self, ctx: &C) {
        if let Some((route, page)) = self.current_mut() {
            page.pause(ctx, route);
        }
    }

    /// Pauses the active page and disposes every page in the stack. Call this
    /// from an outer shell page's [`Page::dispose`].
    pub fn dispose_all(&mut self, ctx: &C) {
        if let Some(mut top) = self.stack.pop() {
            top.page.pause(ctx, &top.route);
            top.page.dispose(ctx, &top.route);
        }

        while let Some(mut entry) = self.stack.pop() {
            entry.page.dispose(ctx, &entry.route);
        }
    }
}
