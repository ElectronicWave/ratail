# Ratail app skeleton

Adapt this skeleton to the project instead of copying it verbatim. Keep the project's existing module layout, naming, and error types.

```rust
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
    redraw: Redraw,
    items: Task<Vec<Item>, String>,
}

impl Services {
    fn new() -> Self {
        let redraw = Redraw::new();
        Self {
            items: Task::with_redraw(&redraw),
            redraw,
        }
    }

    fn reload(&self) {
        self.items.spawn_once(load_items);
    }
}

impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        &self.redraw
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
    fn init(&mut self, ctx: &Services) {
        ctx.reload();
    }

    fn keybindings(&self) -> &[KeyHint] {
        self.keys.hints()
    }

    fn on_key(&mut self, ctx: &Services, key: KeyEvent) -> PageEvent<Route> {
        match self.keys.action_for(&key) {
            Some(HomeAction::Reload) => {
                ctx.reload();
                PageEvent::Handled
            }
            Some(HomeAction::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
            None => PageEvent::Ignored,
        }
    }

    fn render(&mut self, ctx: &Services, frame: &mut Frame, area: Rect) {
        let text = match &*ctx.items.snapshot() {
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
    fn keybindings(&self) -> &[KeyHint] {
        &[]
    }

    fn on_key(&mut self, _ctx: &Services, key: KeyEvent) -> PageEvent<Route> {
        if key.code == KeyCode::Backspace {
            PageEvent::Navigate(NavAction::Pop)
        } else {
            PageEvent::Ignored
        }
    }

    fn render(&mut self, _ctx: &Services, frame: &mut Frame, area: Rect) {
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

- Shared task across routes: keep the `Task` in `Services` and read the same snapshot from each page.
- Page-local task: store the `Task` in the page and create it with `Task::with_redraw(&ctx.redraw)`.
- No action enum: match `KeyEvent` directly in `on_key` and still return `PageEvent`.
- Global key that mutates services: use `App::bind_global` with a closure over `&mut C`; return `AppFlow::Exit` only when the app should stop.
- Error with stale data: render `snapshot.error()` and `snapshot.previous()` together instead of clearing the screen.
