use std::marker::PhantomData;

use ratatui::Frame;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;

use crate::context::Context;
use crate::input::KeyHint;
use crate::navigator::NavAction;
use crate::page::{Page, PageEvent};

/// What a modal wants after handling a key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModalOutcome<T> {
    /// The modal did not consume the key.
    Ignored,
    /// The modal consumed the key and stays open.
    Handled,
    /// The modal was cancelled. It is popped without running its completion
    /// callback.
    Cancelled,
    /// The modal finished and produced a value for its completion callback.
    Completed(T),
}

/// A synchronous overlay rendered above a page.
///
/// The completion value is passed to the callback supplied when the modal is
/// opened with [`ModalHost::open`], so a page does not need to remember what a
/// dialog was for.
pub trait Modal<C: Context> {
    type Value;

    fn on_key(&mut self, ctx: &C, key: KeyEvent) -> ModalOutcome<Self::Value>;

    /// Runs after a redraw when the modal may own async work.
    ///
    /// Returning [`ModalOutcome::Completed`] pops the modal and runs its
    /// completion callback.
    fn poll(&mut self, _ctx: &C) -> ModalOutcome<Self::Value> {
        ModalOutcome::Ignored
    }

    fn render(&mut self, ctx: &C, frame: &mut Frame, area: Rect);

    fn keybindings(&self) -> Vec<KeyHint> {
        Vec::new()
    }
}

/// Converts a modal completion callback return value into a [`PageEvent`].
///
/// Callbacks passed to [`ModalHost::open`] can return:
/// - `()` to stay on the current route (`PageEvent::Handled`),
/// - [`NavAction<R>`] to navigate immediately when the modal completes, or
/// - [`PageEvent<R>`] for explicit control.
pub trait IntoPageEvent<R> {
    fn into_page_event(self) -> PageEvent<R>;
}

impl<R> IntoPageEvent<R> for () {
    fn into_page_event(self) -> PageEvent<R> {
        PageEvent::Handled
    }
}

impl<R> IntoPageEvent<R> for NavAction<R> {
    fn into_page_event(self) -> PageEvent<R> {
        PageEvent::Navigate(self)
    }
}

impl<R> IntoPageEvent<R> for PageEvent<R> {
    fn into_page_event(self) -> PageEvent<R> {
        self
    }
}

/// Owns the active modal stack and each modal's completion callback.
///
/// Chained flows open the next modal from inside the previous callback:
///
/// ```ignore
/// modals.open(FilePicker::new(cwd), |page, _ctx, modals, path| {
///     modals.open(TextPrompt::new("name", default_name(&path)), move |page, ctx, _, name| {
///         ctx.import(page.account_id, name, path);
///     });
/// });
/// ```
///
/// A completion callback may return `()`, [`NavAction<R>`], or
/// [`PageEvent<R>`].
pub struct ModalHost<C: Context, S, R = <C as Context>::Route> {
    stack: Vec<Box<dyn ModalEntry<C, S, R>>>,
}

impl<C: Context, S: 'static, R: 'static> ModalHost<C, S, R> {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Pushes a modal and the callback to run when it completes.
    ///
    /// The callback receives the host so it can open a follow-up modal. Call
    /// `open` from the callback to chain flows without storing modal state on
    /// the page. Returning [`NavAction<R>`] or [`PageEvent<R>`] from the
    /// callback forwards that navigation event out of
    /// [`ModalHost::dispatch_key`] / [`ModalPage`].
    pub fn open<M, F, Out>(&mut self, modal: M, on_complete: F)
    where
        M: Modal<C> + 'static,
        M::Value: 'static,
        F: FnOnce(&mut S, &C, &mut Self, M::Value) -> Out + 'static,
        Out: IntoPageEvent<R> + 'static,
    {
        self.stack.push(Box::new(CallbackModal {
            modal,
            on_complete: Some(on_complete),
            value: None,
            marker: PhantomData,
        }));
    }

    pub fn is_active(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Routes a key to the top modal and returns the resulting [`PageEvent`].
    ///
    /// Returns `None` when no modal is open or the top modal returned
    /// [`ModalOutcome::Ignored`]. When a modal completes, it is popped and its
    /// callback's [`IntoPageEvent`] result (defaulting to
    /// [`PageEvent::Handled`] for `()`) is returned.
    pub fn dispatch_key(&mut self, state: &mut S, ctx: &C, key: KeyEvent) -> Option<PageEvent<R>> {
        let modal = self.stack.last_mut()?;

        match modal.on_key(ctx, key) {
            EntryFlow::Ignored => None,
            EntryFlow::Handled => Some(PageEvent::Handled),
            EntryFlow::Cancelled => {
                self.stack.pop();
                Some(PageEvent::Handled)
            }
            EntryFlow::Completed => {
                let mut modal = self
                    .stack
                    .pop()
                    .expect("the modal stack cannot be empty while completing a modal");
                Some(modal.complete(state, ctx, self))
            }
        }
    }

    /// Routes a key to the top modal.
    ///
    /// Returns `true` when the modal consumed the key. A completed modal is
    /// popped and its callback runs before this method returns; a cancelled
    /// modal is popped without calling its callback. `false` means the key was
    /// ignored, so the page may handle it.
    pub fn on_key(&mut self, state: &mut S, ctx: &C, key: KeyEvent) -> bool {
        self.dispatch_key(state, ctx, key).is_some()
    }

    /// Polls the top modal.
    ///
    /// Returns a navigation action when an asynchronously completed modal runs
    /// a callback that navigates.
    pub fn poll(&mut self, state: &mut S, ctx: &C) -> Option<NavAction<R>> {
        let flow = self.stack.last_mut()?.poll(ctx);

        match flow {
            EntryFlow::Ignored | EntryFlow::Handled => None,
            EntryFlow::Cancelled => {
                self.stack.pop();
                None
            }
            EntryFlow::Completed => {
                let mut modal = self
                    .stack
                    .pop()
                    .expect("the modal stack cannot be empty while completing a modal");
                match modal.complete(state, ctx, self) {
                    PageEvent::Navigate(action) => Some(action),
                    PageEvent::Ignored | PageEvent::Handled => None,
                }
            }
        }
    }

    /// Renders every active modal in stack order, with newer modals on top.
    pub fn render(&mut self, ctx: &C, frame: &mut Frame, area: Rect) {
        for modal in &mut self.stack {
            modal.render(ctx, frame, area);
        }
    }

    /// Returns the top modal's hints, or `None` when no modal is open.
    pub fn keybindings(&self) -> Option<Vec<KeyHint>> {
        self.stack.last().map(|modal| modal.keybindings())
    }

    /// Drops every modal without running completion callbacks.
    pub fn clear(&mut self) {
        self.stack.clear();
    }
}

impl<C: Context, S: 'static, R: 'static> Default for ModalHost<C, S, R> {
    fn default() -> Self {
        Self::new()
    }
}

/// A page whose modal stack is managed automatically by [`ModalPage`].
///
/// Implementing `WithModal` instead of [`Page`] lets the page struct pass
/// `&mut Self` directly to modal completion callbacks—no need to split the page
/// into a separate `State` struct and a `ModalHost` field, and no boilerplate
/// to forward `on_key`, `render`, or `keybindings` to the modal stack.
pub trait WithModal<C: Context, R = <C as Context>::Route>: Sized + 'static {
    fn init(&mut self, _ctx: &C, _route: &R, _modals: &mut ModalHost<C, Self, R>) {}

    fn resume(&mut self, _ctx: &C, _route: &R, _modals: &mut ModalHost<C, Self, R>) {}

    fn pause(&mut self, _ctx: &C, _route: &R, _modals: &mut ModalHost<C, Self, R>) {}

    fn dispose(&mut self, _ctx: &C, _route: &R, _modals: &mut ModalHost<C, Self, R>) {}

    fn on_key(
        &mut self,
        _ctx: &C,
        _route: &R,
        _modals: &mut ModalHost<C, Self, R>,
        _key: KeyEvent,
    ) -> PageEvent<R> {
        PageEvent::Ignored
    }

    fn poll(
        &mut self,
        _ctx: &C,
        _route: &R,
        _modals: &mut ModalHost<C, Self, R>,
    ) -> Option<NavAction<R>> {
        None
    }

    fn keybindings(&self, _route: &R) -> Vec<KeyHint> {
        Vec::new()
    }

    fn render(&mut self, ctx: &C, route: &R, frame: &mut Frame, area: Rect);
}

/// Wraps a [`WithModal`] state in a [`Page`] that automatically intercepts
/// keys, overlays active modals on render, and substitutes the top modal's key
/// hints while a modal is open.
pub struct ModalPage<C: Context, P: WithModal<C, R>, R = <C as Context>::Route> {
    page: P,
    modals: ModalHost<C, P, R>,
}

impl<C: Context, P: WithModal<C, R>, R: 'static> ModalPage<C, P, R> {
    pub fn new(page: P) -> Self {
        Self {
            page,
            modals: ModalHost::new(),
        }
    }

    pub fn page(&self) -> &P {
        &self.page
    }

    pub fn page_mut(&mut self) -> &mut P {
        &mut self.page
    }

    pub fn modals(&self) -> &ModalHost<C, P, R> {
        &self.modals
    }

    pub fn modals_mut(&mut self) -> &mut ModalHost<C, P, R> {
        &mut self.modals
    }
}

impl<C: Context, P: WithModal<C, R>, R: 'static> Page<C, R> for ModalPage<C, P, R> {
    fn init(&mut self, ctx: &C, route: &R) {
        self.page.init(ctx, route, &mut self.modals);
    }

    fn resume(&mut self, ctx: &C, route: &R) {
        self.page.resume(ctx, route, &mut self.modals);
    }

    fn pause(&mut self, ctx: &C, route: &R) {
        self.page.pause(ctx, route, &mut self.modals);
    }

    fn dispose(&mut self, ctx: &C, route: &R) {
        self.modals.clear();
        self.page.dispose(ctx, route, &mut self.modals);
    }

    fn on_key(&mut self, ctx: &C, route: &R, key: KeyEvent) -> PageEvent<R> {
        if let Some(event) = self.modals.dispatch_key(&mut self.page, ctx, key) {
            return event;
        }
        self.page.on_key(ctx, route, &mut self.modals, key)
    }

    fn poll(&mut self, ctx: &C, route: &R) -> Option<NavAction<R>> {
        if let Some(action) = self.modals.poll(&mut self.page, ctx) {
            return Some(action);
        }
        self.page.poll(ctx, route, &mut self.modals)
    }

    fn keybindings(&self, route: &R) -> Vec<KeyHint> {
        self.modals
            .keybindings()
            .unwrap_or_else(|| self.page.keybindings(route))
    }

    fn render(&mut self, ctx: &C, route: &R, frame: &mut Frame, area: Rect) {
        self.page.render(ctx, route, frame, area);
        self.modals.render(ctx, frame, area);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryFlow {
    Ignored,
    Handled,
    Cancelled,
    Completed,
}

trait ModalEntry<C: Context, S, R> {
    fn on_key(&mut self, ctx: &C, key: KeyEvent) -> EntryFlow;
    fn poll(&mut self, ctx: &C) -> EntryFlow;
    fn complete(&mut self, state: &mut S, ctx: &C, host: &mut ModalHost<C, S, R>) -> PageEvent<R>;
    fn render(&mut self, ctx: &C, frame: &mut Frame, area: Rect);
    fn keybindings(&self) -> Vec<KeyHint>;
}

struct CallbackModal<C: Context, S, R, M: Modal<C>, F, Out> {
    modal: M,
    on_complete: Option<F>,
    value: Option<M::Value>,
    marker: PhantomData<fn(&mut S, &C, &R) -> Out>,
}

impl<C: Context, S, R, M, F, Out> ModalEntry<C, S, R> for CallbackModal<C, S, R, M, F, Out>
where
    M: Modal<C>,
    F: FnOnce(&mut S, &C, &mut ModalHost<C, S, R>, M::Value) -> Out,
    Out: IntoPageEvent<R>,
{
    fn on_key(&mut self, ctx: &C, key: KeyEvent) -> EntryFlow {
        match self.modal.on_key(ctx, key) {
            ModalOutcome::Ignored => EntryFlow::Ignored,
            ModalOutcome::Handled => EntryFlow::Handled,
            ModalOutcome::Cancelled => EntryFlow::Cancelled,
            ModalOutcome::Completed(value) => {
                self.value = Some(value);
                EntryFlow::Completed
            }
        }
    }

    fn poll(&mut self, ctx: &C) -> EntryFlow {
        match self.modal.poll(ctx) {
            ModalOutcome::Ignored => EntryFlow::Ignored,
            ModalOutcome::Handled => EntryFlow::Handled,
            ModalOutcome::Cancelled => EntryFlow::Cancelled,
            ModalOutcome::Completed(value) => {
                self.value = Some(value);
                EntryFlow::Completed
            }
        }
    }

    fn complete(&mut self, state: &mut S, ctx: &C, host: &mut ModalHost<C, S, R>) -> PageEvent<R> {
        let value = self
            .value
            .take()
            .expect("a completed modal must hold its completion value");
        let on_complete = self
            .on_complete
            .take()
            .expect("a completed modal must hold its completion callback");
        on_complete(state, ctx, host, value).into_page_event()
    }

    fn render(&mut self, ctx: &C, frame: &mut Frame, area: Rect) {
        self.modal.render(ctx, frame, area);
    }

    fn keybindings(&self) -> Vec<KeyHint> {
        self.modal.keybindings()
    }
}
