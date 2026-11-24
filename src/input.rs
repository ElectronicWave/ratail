use std::borrow::Cow;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};

const DEFAULT_DOUBLE_CLICK_THRESHOLD: Duration = Duration::from_millis(350);

/// A normalized key press. Release events are ignored; press and repeat are kept
/// so holding a key keeps repeating its action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn plain(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }

    pub fn char(character: char) -> Self {
        Self::plain(KeyCode::Char(character))
    }

    pub fn ctrl(character: char) -> Self {
        Self::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    pub fn from_event(event: &KeyEvent) -> Option<Self> {
        if event.kind == KeyEventKind::Release {
            return None;
        }

        Some(Self {
            code: event.code,
            modifiers: event.modifiers,
        })
    }
}

/// A key binding hint that can be rendered in a footer.
///
/// `key` and `description` accept owned or borrowed text, so translations
/// produced by crates such as `rust-i18n` can be used directly:
/// `rust_i18n::t!(...)` returns `Cow<'static, str>` and fits both arguments.
///
/// ```
/// use ratail::KeyHint;
///
/// let hint = KeyHint::new("Ctrl+Q", String::from("quit"));
/// assert_eq!(hint.description, "quit");
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyHint {
    pub key: Cow<'static, str>,
    pub description: Cow<'static, str>,
}

impl KeyHint {
    pub fn new(
        key: impl Into<Cow<'static, str>>,
        description: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
        }
    }

    /// Creates a hint from static text without allocating.
    pub const fn from_static(key: &'static str, description: &'static str) -> Self {
        Self {
            key: Cow::Borrowed(key),
            description: Cow::Borrowed(description),
        }
    }
}

struct Binding<A> {
    chord: KeyChord,
    action: A,
}

/// A key map owned by a page or a focus scope.
pub struct KeyMap<A> {
    bindings: Vec<Binding<A>>,
    hints: Vec<KeyHint>,
}

impl<A> Default for KeyMap<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> KeyMap<A> {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            hints: Vec::new(),
        }
    }

    pub fn bind(&mut self, chord: KeyChord, action: A) {
        assert!(
            !self.bindings.iter().any(|binding| binding.chord == chord),
            "duplicate key binding for {chord:?}"
        );

        self.bindings.push(Binding { chord, action });
    }

    pub fn bind_with_hint(&mut self, chord: KeyChord, action: A, hint: KeyHint) {
        self.bind(chord, action);
        self.hints.push(hint);
    }

    pub fn hints(&self) -> &[KeyHint] {
        &self.hints
    }
}

impl<A: Copy> KeyMap<A> {
    pub fn action(&self, chord: KeyChord) -> Option<A> {
        self.bindings
            .iter()
            .find(|binding| binding.chord == chord)
            .map(|binding| binding.action)
    }

    /// Resolves a key input event.
    pub fn action_for(&self, event: &InputEvent) -> Option<A> {
        event.key().and_then(|chord| self.action(chord))
    }
}

/// A small focus ring for Tab / Shift+Tab style navigation.
pub struct FocusRing<F: Copy + Eq> {
    items: Vec<F>,
    current: usize,
}

impl<F: Copy + Eq> FocusRing<F> {
    pub fn new(items: Vec<F>) -> Self {
        assert!(
            !items.is_empty(),
            "focus ring must contain at least one item"
        );

        Self { items, current: 0 }
    }

    pub fn current(&self) -> F {
        self.items[self.current]
    }

    pub fn next(&mut self) {
        self.current = (self.current + 1) % self.items.len();
    }

    pub fn previous(&mut self) {
        self.current = if self.current == 0 {
            self.items.len() - 1
        } else {
            self.current - 1
        };
    }

    pub fn focus(&mut self, focus: F) -> bool {
        if let Some(index) = self.items.iter().position(|item| *item == focus) {
            self.current = index;
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A normalized mouse action produced by [`App`](crate::App).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MouseKind {
    Click,
    DoubleClick,
    RightClick,
    MiddleClick,
    Drag(MouseButton),
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

/// A normalized mouse event delivered inside [`InputEvent::Mouse`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct MouseInput {
    pub kind: MouseKind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}

/// A normalized terminal input event routed to pages and modals.
///
/// [`App`](crate::App) filters key releases and pointer motion noise before
/// dispatching `InputEvent`, and tracks double clicks globally across the
/// terminal pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputEvent {
    Key(KeyChord),
    Mouse(MouseInput),
    Paste(String),
}

impl InputEvent {
    pub fn key(&self) -> Option<KeyChord> {
        match self {
            Self::Key(chord) => Some(*chord),
            Self::Mouse(_) | Self::Paste(_) => None,
        }
    }

    pub fn mouse(&self) -> Option<MouseInput> {
        match self {
            Self::Mouse(mouse) => Some(*mouse),
            Self::Key(_) | Self::Paste(_) => None,
        }
    }

    pub fn paste(&self) -> Option<&str> {
        match self {
            Self::Paste(text) => Some(text.as_str()),
            Self::Key(_) | Self::Mouse(_) => None,
        }
    }
}

/// A region matched by [`HitMap`], together with pointer coordinates relative to
/// that region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hit<T> {
    pub id: T,
    pub area: Rect,
    /// Zero-based column offset from `area.x`.
    pub column: u16,
    /// Zero-based row offset from `area.y`.
    pub row: u16,
}

impl<T> Hit<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Hit<U> {
        Hit {
            id: f(self.id),
            area: self.area,
            column: self.column,
            row: self.row,
        }
    }
}

/// Per-frame rectangular hit regions recorded during `render` and queried in
/// `on_input`.
///
/// Regions registered later take precedence over earlier regions, matching
/// draw order when overlays or nested panels overlap.
///
/// ```
/// use ratail::HitMap;
/// use ratatui::layout::Rect;
///
/// let mut hits = HitMap::new();
/// hits.register(Rect::new(0, 0, 20, 10), "panel");
/// hits.register(Rect::new(5, 2, 10, 4), "popup");
///
/// let hit = hits.hit_copied(7, 3).unwrap();
/// assert_eq!(hit.id, "popup");
/// assert_eq!((hit.column, hit.row), (2, 1));
/// ```
#[derive(Clone, Debug)]
pub struct HitMap<T> {
    entries: Vec<(Rect, T)>,
}

impl<T> Default for HitMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> HitMap<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Records a hit region when `area` is non-empty.
    pub fn register(&mut self, area: Rect, id: T) {
        if !area.is_empty() {
            self.entries.push((area, id));
        }
    }

    /// Returns the topmost region containing `(column, row)`.
    pub fn hit(&self, column: u16, row: u16) -> Option<Hit<&T>> {
        let point = Position::new(column, row);
        self.entries
            .iter()
            .rev()
            .find(|(area, _)| area.contains(point))
            .map(|(area, id)| Hit {
                id,
                area: *area,
                column: column - area.x,
                row: row - area.y,
            })
    }

    pub fn hit_mouse(&self, mouse: MouseInput) -> Option<Hit<&T>> {
        self.hit(mouse.column, mouse.row)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T: Copy> HitMap<T> {
    pub fn hit_copied(&self, column: u16, row: u16) -> Option<Hit<T>> {
        self.hit(column, row).map(|hit| hit.map(|id| *id))
    }

    pub fn hit_mouse_copied(&self, mouse: MouseInput) -> Option<Hit<T>> {
        self.hit_copied(mouse.column, mouse.row)
    }
}

#[derive(Clone, Copy, Debug)]
struct LastClick {
    column: u16,
    row: u16,
    at: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct MouseNormalizer {
    threshold: Duration,
    last_click: Option<LastClick>,
}

impl Default for MouseNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseNormalizer {
    pub(crate) fn new() -> Self {
        Self {
            threshold: DEFAULT_DOUBLE_CLICK_THRESHOLD,
            last_click: None,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.last_click = None;
    }

    pub(crate) fn normalize(&mut self, event: &MouseEvent) -> Option<MouseInput> {
        let now = Instant::now();
        let kind = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(previous) = self.last_click.take()
                    && previous.column == event.column
                    && previous.row == event.row
                    && now.saturating_duration_since(previous.at) <= self.threshold
                {
                    MouseKind::DoubleClick
                } else {
                    self.last_click = Some(LastClick {
                        column: event.column,
                        row: event.row,
                        at: now,
                    });
                    MouseKind::Click
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.last_click = None;
                MouseKind::RightClick
            }
            MouseEventKind::Down(MouseButton::Middle) => {
                self.last_click = None;
                MouseKind::MiddleClick
            }
            MouseEventKind::Drag(button) => {
                if button != MouseButton::Left
                    || self
                        .last_click
                        .is_some_and(|prev| prev.column != event.column || prev.row != event.row)
                {
                    self.last_click = None;
                }
                MouseKind::Drag(button)
            }
            MouseEventKind::ScrollUp => {
                self.last_click = None;
                MouseKind::ScrollUp
            }
            MouseEventKind::ScrollDown => {
                self.last_click = None;
                MouseKind::ScrollDown
            }
            MouseEventKind::ScrollLeft => {
                self.last_click = None;
                MouseKind::ScrollLeft
            }
            MouseEventKind::ScrollRight => {
                self.last_click = None;
                MouseKind::ScrollRight
            }
            MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Moved => return None,
            MouseEventKind::Up(_) => {
                self.last_click = None;
                return None;
            }
        };

        Some(MouseInput {
            kind,
            column: event.column,
            row: event.row,
            modifiers: event.modifiers,
        })
    }
}
