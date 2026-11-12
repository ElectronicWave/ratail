use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

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

/// A key binding hint that can be rendered in a title bar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyHint {
    pub key: &'static str,
    pub description: &'static str,
}

impl KeyHint {
    pub const fn new(key: &'static str, description: &'static str) -> Self {
        Self { key, description }
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

    /// Resolves a key event, ignoring release events.
    pub fn action_for(&self, event: &KeyEvent) -> Option<A> {
        KeyChord::from_event(event).and_then(|chord| self.action(chord))
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
