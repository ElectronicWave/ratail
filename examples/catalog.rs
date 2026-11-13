//! Shared async state demo.
//!
//! Both routes read the same [`Task`]. Press `r` on either page: the request
//! runs once, and every page renders the same snapshot. Every fourth request
//! fails on purpose so the error path and previous-value retention are visible.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ratail::{
    App, AppFlow, Context, KeyChord, KeyHint, KeyMap, NavAction, Page, PageEvent, Redraw, Router,
    Snapshot, Task,
};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

type Catalog = (usize, Vec<String>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Dashboard,
    Details,
}

#[derive(Clone, Copy)]
enum Action {
    Reload,
    Details,
    Back,
}

struct Services {
    redraw: Redraw,
    catalog: Task<Catalog, String>,
    requests: AtomicUsize,
}

impl Services {
    fn new() -> Self {
        let redraw = Redraw::new();
        Self {
            catalog: Task::with_redraw(&redraw),
            redraw,
            requests: AtomicUsize::new(0),
        }
    }

    fn reload(&self) {
        if self.catalog.is_running() {
            return;
        }

        let request = self.requests.fetch_add(1, Ordering::Relaxed) + 1;
        self.catalog.spawn_once(move || load_catalog(request));
    }
}

impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        &self.redraw
    }
}

async fn load_catalog(request: usize) -> Result<Catalog, String> {
    tokio::time::sleep(Duration::from_millis(900)).await;

    if request % 4 == 3 {
        return Err(format!("simulated backend failure for request {request}"));
    }

    Ok((request, (1..=5).map(|i| format!("module-{i}")).collect()))
}

struct AppRouter;

impl Router<Services> for AppRouter {
    fn create(&mut self, route: &Route, _ctx: &Services) -> Box<dyn Page<Services>> {
        Box::new(CatalogPage::new(*route))
    }
}

struct CatalogPage {
    route: Route,
    keys: KeyMap<Action>,
}

impl CatalogPage {
    fn new(route: Route) -> Self {
        let mut keys = KeyMap::new();
        match route {
            Route::Dashboard => {
                bind(
                    &mut keys,
                    KeyChord::char('d'),
                    Action::Details,
                    "d",
                    "details",
                );
            }
            Route::Details => {
                bind(
                    &mut keys,
                    KeyChord::plain(KeyCode::Backspace),
                    Action::Back,
                    "Backspace",
                    "back",
                );
            }
        }
        bind(
            &mut keys,
            KeyChord::char('r'),
            Action::Reload,
            "r",
            "reload",
        );

        Self { route, keys }
    }
}

fn bind(
    keys: &mut KeyMap<Action>,
    chord: KeyChord,
    action: Action,
    key: &'static str,
    description: &'static str,
) {
    keys.bind_with_hint(chord, action, KeyHint::new(key, description));
}

impl Page<Services> for CatalogPage {
    fn init(&mut self, ctx: &Services) {
        if self.route == Route::Dashboard {
            ctx.reload();
        }
    }

    fn keybindings(&self) -> Vec<KeyHint> {
        self.keys.hints().to_vec()
    }

    fn on_key(&mut self, ctx: &Services, key: KeyEvent) -> PageEvent<Route> {
        match self.keys.action_for(&key) {
            Some(Action::Reload) => {
                ctx.reload();
                PageEvent::Handled
            }
            Some(Action::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
            Some(Action::Back) => PageEvent::Navigate(NavAction::Pop),
            None => PageEvent::Ignored,
        }
    }

    fn render(&mut self, ctx: &Services, frame: &mut Frame, area: Rect) {
        let mut lines = vec![Line::from(vec![
            Span::styled("Catalog", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "   shared requests: {}",
                ctx.requests.load(Ordering::Relaxed)
            )),
        ])];
        lines.push(Line::default());
        lines.extend(catalog_lines(&ctx.catalog.snapshot()));
        frame.render_widget(Paragraph::new(lines).block(Block::bordered()), area);
    }
}

fn catalog_lines(snapshot: &Snapshot<Catalog, String>) -> Vec<Line<'static>> {
    let mut lines = match snapshot {
        Snapshot::Idle => vec![Line::from("not started")],
        Snapshot::Loading { .. } => vec![Line::from("loading...")],
        Snapshot::Ready(catalog) => return catalog_body(catalog),
        Snapshot::Error { error, .. } => vec![Line::from(format!("error: {error}"))],
        Snapshot::Cancelled { .. } => vec![Line::from("cancelled")],
    };

    match snapshot.previous() {
        Some(catalog) => {
            lines.push(Line::from(format!("keeping request {}", catalog.0)));
            lines.extend(catalog_body(catalog));
        }
        None => lines.push(Line::from("no previous result yet")),
    }
    lines
}

fn catalog_body((request, items): &Catalog) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!(
        "request {request} · {} modules",
        items.len()
    ))];
    lines.extend(
        items
            .iter()
            .map(|name| Line::from(format!("  {name}  (updated in request {request})"))),
    );
    lines
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let mut app = App::new(Services::new(), AppRouter, Route::Dashboard);
    app.bind_global(KeyChord::ctrl('q'), KeyHint::new("Ctrl+Q", "quit"), |_| {
        AppFlow::Exit
    });

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal).await;
    ratatui::restore();
    result
}
