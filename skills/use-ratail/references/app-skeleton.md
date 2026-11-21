# Ratail app skeleton

Adapt this skeleton to the project instead of copying it verbatim. Keep the project's existing module layout, naming, and error types. A small single-task app can keep a flat `Services`; this skeleton shows the split used as the app grows.

```rust
use std::sync::Arc;
use std::time::Duration;

use ratail::{
    App, AppFlow, Context, KeyChord, KeyHint, KeyMap, NavAction, Page, PageEvent, Redraw, Router,
    Snapshot, Task,
};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

type Item = String;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Home,
    Details,
}

struct Services {
    ui: UiService,
    items: ItemService,
}

impl Services {
    fn new() -> Self {
        let ui = UiService::new();
        let items = ItemService::new(ui.redraw());
        Self { ui, items }
    }

    fn items(&self) -> &ItemService {
        &self.items
    }
}

impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        self.ui.redraw()
    }
}

struct UiService {
    redraw: Redraw,
}

impl UiService {
    fn new() -> Self {
        Self {
            redraw: Redraw::new(),
        }
    }

    fn redraw(&self) -> &Redraw {
        &self.redraw
    }
}

struct ItemService {
    items: Task<Vec<Item>, String>,
}

impl ItemService {
    fn new(redraw: &Redraw) -> Self {
        Self {
            items: Task::with_redraw(redraw),
        }
    }

    fn snapshot(&self) -> Arc<Snapshot<Vec<Item>, String>> {
        self.items.snapshot()
    }

    fn reload(&self) {
        self.items.spawn_once(load_items);
    }
}

async fn load_items() -> Result<Vec<Item>, String> {
    tokio::time::sleep(Duration::from_millis(400)).await;
    Ok(vec!["alpha".to_string(), "beta".to_string()])
}

struct AppRouter;

impl Router<Services> for AppRouter {
    fn create(&mut self, route: &Route, _ctx: &Services) -> Box<dyn Page<Services>> {
        match route {
            Route::Home => Box::new(HomePage::new()),
            Route::Details => Box::new(DetailsPage),
        }
    }
}

#[derive(Clone, Copy)]
enum HomeAction {
    Reload,
    Details,
}

struct HomePage {
    keys: KeyMap<HomeAction>,
}

impl HomePage {
    fn new() -> Self {
        let mut keys = KeyMap::new();
        keys.bind_with_hint(
            KeyChord::char('r'),
            HomeAction::Reload,
            KeyHint::new("r", "reload"),
        );
        keys.bind_with_hint(
            KeyChord::char('d'),
            HomeAction::Details,
            KeyHint::new("d", "details"),
        );

        Self { keys }
    }
}

impl Page<Services> for HomePage {
    fn init(&mut self, ctx: &Services, _route: &Route) {
        ctx.items().reload();
    }

    fn keybindings(&self, _route: &Route) -> Vec<KeyHint> {
        self.keys.hints().to_vec()
    }

    fn on_key(&mut self, ctx: &Services, _route: &Route, key: KeyEvent) -> PageEvent<Route> {
        match self.keys.action_for(&key) {
            Some(HomeAction::Reload) => {
                ctx.items().reload();
                PageEvent::Handled
            }
            Some(HomeAction::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
            None => PageEvent::Ignored,
        }
    }

    fn render(&mut self, ctx: &Services, _route: &Route, frame: &mut Frame, area: Rect) {
        let text = match &*ctx.items().snapshot() {
            Snapshot::Idle => "idle".to_string(),
            Snapshot::Loading { .. } => "loading".to_string(),
            Snapshot::Ready(items) => format!("{} items", items.len()),
            Snapshot::Error { error, .. } => format!("error: {error}"),
            Snapshot::Cancelled { .. } => "cancelled".to_string(),
        };

        frame.render_widget(Paragraph::new(text), area);
    }
}

struct DetailsPage;

impl Page<Services> for DetailsPage {
    fn keybindings(&self, _route: &Route) -> Vec<KeyHint> {
        Vec::new()
    }

    fn on_key(&mut self, _ctx: &Services, _route: &Route, key: KeyEvent) -> PageEvent<Route> {
        if key.code == KeyCode::Backspace {
            PageEvent::Navigate(NavAction::Pop)
        } else {
            PageEvent::Ignored
        }
    }

    fn render(&mut self, _ctx: &Services, _route: &Route, frame: &mut Frame, area: Rect) {
        frame.render_widget(Paragraph::new("details"), area);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let mut app = App::new(Services::new(), AppRouter, Route::Home);
    app.bind_global(KeyChord::ctrl('q'), KeyHint::new("Ctrl+Q", "quit"), |_| {
        AppFlow::Exit
    });

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal).await;
    ratatui::restore();
    result
}
```

## Variations

- Shared task across routes: keep the `Task` in its domain service and read the same snapshot from each page.
- Parameterized request: store a `TaskCache<K, T, E>` in the domain service when each key needs its own snapshot, such as a catalog per driver. The cache is LRU-bounded (32 entries by default); use `TaskCache::with_capacity` when that bound does not fit.
- New domain area: add a service struct with private fields, construct it in `Services::new`, inject the shared `Redraw`, and expose the service through an accessor.
- Page-local task: store the `Task` in the page and create it with `Task::with_redraw(ctx.redraw())`.
- No action enum: match `KeyEvent` directly in `on_key` and still return `PageEvent`.
- Global key that mutates services: use `App::bind_global` with a closure over `&mut C`; return `AppFlow::Exit` only when the app should stop.
- Custom footer: call `App::set_hint_renderer` and draw any widget in the footer area; `hint_line` builds the default spans.
- Error with stale data: render `snapshot.error()` and `snapshot.previous()` together instead of clearing the screen.
- Nested section: give the shell a `Navigator<Services, SectionRoute, SectionRouter>` and delegate with `nav.handle_key(ctx, key)`, `nav.render(ctx, frame, area)`, `nav.keybindings()`, and `nav.dispose_all(ctx)`.
- Modal flow: implement `WithModal<Services, Route>` and wrap the page in `ModalPage::new(page)` so callbacks receive `&mut Self` and can chain modals or return `NavAction` without splitting page state.
