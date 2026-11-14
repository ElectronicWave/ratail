use crate::context::Context;
use crate::page::Page;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NavAction<R> {
    Push(R),
    Pop,
    Replace(R),
    Reset(R),
}

/// Creates page state for a route.
pub trait Router<C: Context> {
    fn create(&mut self, route: &C::Route, ctx: &C) -> Box<dyn Page<C>>;
}

pub struct Navigator<C: Context, R: Router<C>> {
    router: R,
    stack: Vec<RouteEntry<C>>,
}

struct RouteEntry<C: Context> {
    route: C::Route,
    page: Box<dyn Page<C>>,
}

impl<C: Context, R: Router<C>> Navigator<C, R> {
    pub fn new(mut router: R, root: C::Route, ctx: &C) -> Self {
        let mut page = router.create(&root, ctx);
        page.init(ctx, &root);
        page.resume(ctx, &root);

        Self {
            router,
            stack: vec![RouteEntry { route: root, page }],
        }
    }

    pub fn apply(&mut self, action: NavAction<C::Route>, ctx: &C) {
        match action {
            NavAction::Push(route) => self.push(route, ctx),
            NavAction::Pop => {
                self.pop(ctx);
            }
            NavAction::Replace(route) => self.replace(route, ctx),
            NavAction::Reset(route) => self.reset(route, ctx),
        }
    }

    pub fn push(&mut self, route: C::Route, ctx: &C) {
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

    pub fn replace(&mut self, route: C::Route, ctx: &C) {
        if let Some(mut current) = self.stack.pop() {
            current.page.pause(ctx, &current.route);
            current.page.dispose(ctx, &current.route);
        }

        let mut page = self.router.create(&route, ctx);
        page.init(ctx, &route);
        page.resume(ctx, &route);

        self.stack.push(RouteEntry { route, page });
    }

    pub fn reset(&mut self, route: C::Route, ctx: &C) {
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
    pub fn current(&self) -> Option<(&C::Route, &(dyn Page<C> + 'static))> {
        self.stack
            .last()
            .map(|entry| (&entry.route, entry.page.as_ref()))
    }

    /// Mutably borrows the current route together with its page.
    pub fn current_mut(&mut self) -> Option<(&C::Route, &mut (dyn Page<C> + 'static))> {
        self.stack
            .last_mut()
            .map(|entry| (&entry.route, entry.page.as_mut()))
    }

    pub fn current_route(&self) -> Option<&C::Route> {
        self.stack.last().map(|entry| &entry.route)
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}
