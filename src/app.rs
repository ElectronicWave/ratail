use std::future::poll_fn;

use futures_core::Stream;
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::crossterm::event::{Event, EventStream, KeyEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::context::Context;
use crate::input::{KeyChord, KeyHint};
use crate::navigator::{Navigator, Router};
use crate::page::PageEvent;

/// Controls the application loop after a global key action runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppFlow {
    Continue,
    Exit,
}

pub struct App<C: Context, R: Router<C>> {
    context: C,
    navigator: Navigator<C, R>,
    globals: Vec<GlobalBinding<C>>,
}

struct GlobalBinding<C: Context> {
    chord: KeyChord,
    hint: KeyHint,
    action: Box<dyn FnMut(&mut C) -> AppFlow>,
}

impl<C: Context, R: Router<C>> App<C, R> {
    pub fn new(context: C, router: R, root: C::Route) -> Self {
        let navigator = Navigator::new(router, root, &context);

        Self {
            context,
            navigator,
            globals: Vec::new(),
        }
    }

    pub fn context(&self) -> &C {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut C {
        &mut self.context
    }

    pub fn navigator(&self) -> &Navigator<C, R> {
        &self.navigator
    }

    pub fn navigator_mut(&mut self) -> &mut Navigator<C, R> {
        &mut self.navigator
    }

    /// Binds a key handled after the current page ignores it.
    pub fn bind_global<F>(&mut self, chord: KeyChord, hint: KeyHint, action: F)
    where
        F: FnMut(&mut C) -> AppFlow + 'static,
    {
        self.globals.push(GlobalBinding {
            chord,
            hint,
            action: Box::new(action),
        });
    }

    /// Runs the application on the current tokio runtime.
    ///
    /// The loop wakes on terminal events and on task repaint signals, so there
    /// is no frame interval to tune.
    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        let mut events = EventStream::new();

        loop {
            terminal.draw(|frame| self.render(frame))?;

            let redraw = self.context.redraw().clone();
            tokio::select! {
                event = poll_fn(|cx| std::pin::Pin::new(&mut events).poll_next(cx)) => {
                    match event {
                        Some(Ok(Event::Key(key))) => {
                            if self.handle_key(key) == AppFlow::Exit {
                                break;
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => return Err(error),
                        None => break,
                    }
                }
                _ = redraw.requested() => {}
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> AppFlow {
        let chord = KeyChord::from_event(&key);
        let event = match self.navigator.current_mut() {
            Some(page) => page.on_key(&self.context, key),
            None => PageEvent::Ignored,
        };

        match event {
            PageEvent::Ignored => {}
            PageEvent::Handled => return AppFlow::Continue,
            PageEvent::Navigate(action) => {
                self.navigator.apply(action, &self.context);
                return AppFlow::Continue;
            }
        }

        let Some(chord) = chord else {
            return AppFlow::Continue;
        };
        let Some(index) = self
            .globals
            .iter()
            .position(|binding| binding.chord == chord)
        else {
            return AppFlow::Continue;
        };

        (self.globals[index].action)(&mut self.context)
    }

    fn render(&mut self, frame: &mut Frame) {
        let hints = self.hints();
        let area = frame.area();

        if hints.is_empty() {
            if let Some(page) = self.navigator.current_mut() {
                page.render(&self.context, frame, area);
            }
            return;
        }

        let [body, footer] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

        if let Some(page) = self.navigator.current_mut() {
            page.render(&self.context, frame, body);
        }
        frame.render_widget(Paragraph::new(hints_line(&hints)), footer);
    }

    fn hints(&self) -> Vec<KeyHint> {
        let mut hints = Vec::new();
        if let Some(page) = self.navigator.current() {
            hints.extend_from_slice(page.keybindings());
        }
        hints.extend(self.globals.iter().map(|binding| binding.hint));
        hints
    }
}

fn hints_line(hints: &[KeyHint]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, hint) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            hint.key,
            Style::default().add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(format!(" {}", hint.description)));
    }
    Line::from(spans)
}
