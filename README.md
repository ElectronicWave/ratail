# ratail

English | [简体中文](README.zh.md)

Lightweight async UI framework for ratatui. Pages are synchronous, state is
route-local, and async results are observable; there is no global action bus or
fixed frame rate.

## Install

```toml
[dependencies]
ratail = { path = "path/to/ratail" }
ratatui = "0.30"
tokio = { version = "1", features = ["macros", "rt", "time"] }
```

> [!NOTE]
> ratail only runs on tokio. Pages are synchronous; async work lives in
> `Task`s stored in the `Context`.

## Architecture

```text
terminal input ─┐
                ├─> App::run ─> Navigator ─> active Page
Redraw ─────────┘                 └─ on_input / poll / render

Task completes -> update snapshot -> Redraw.trigger()
```

`App::run` has two wake sources: terminal events and `Redraw`. Normalized keys,
mouse events inside the body (enable capture with `App::enable_mouse`), and
pastes go to the active page's `on_input`; async state changes go to `poll`.
Each redraw polls the active page at most once and applies at most one
`NavAction`.

Where state lives:

- **Page-local state** belongs to one route instance and disappears when the page is removed.
- **`Context` services** are shared across pages and accessed through a read-only `&C` plus domain methods.
- **`Task` / `TaskCache`** hold async results; render reads snapshots, while poll performs one-shot transitions.

The result is **federated MVU plus immediate-mode rendering**: each page owns its
own state transitions, with no central model, global action, or virtual DOM.

## Quick start

The snippets below omit some imports; the complete example is in
[`examples/catalog.rs`](examples/catalog.rs).

```rust
struct Services {
    redraw: Redraw,
    catalog: Task<Catalog, String>,
}

impl Services {
    fn new() -> Self {
        let redraw = Redraw::new();
        let catalog = Task::with_redraw(&redraw);
        Self { redraw, catalog }
    }

    fn reload(&self) {
        self.catalog.spawn_once(load_catalog);
    }
}

impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        &self.redraw
    }
}
```

```rust
struct Home;

impl Page<Services> for Home {
    fn init(&mut self, ctx: &Services, _route: &Route) {
        ctx.reload();
    }

    fn on_input(&mut self, ctx: &Services, _route: &Route, input: InputEvent) -> PageEvent<Route> {
        match input {
            InputEvent::Key(key) if key == KeyChord::char('r') => {
                ctx.reload();
                PageEvent::Handled
            }
            _ => PageEvent::Ignored,
        }
    }

    fn render(&mut self, ctx: &Services, _route: &Route, frame: &mut Frame, area: Rect) {
        let snapshot = ctx.catalog.snapshot();
        let text = match &*snapshot {
            Snapshot::Ready(value) => value.as_str(),
            Snapshot::Error { error, .. } => error.as_str(),
            _ => "loading...",
        };
        frame.render_widget(Paragraph::new(text), area);
    }
}
```

```rust
struct AppRouter;

impl Router<Services> for AppRouter {
    fn create(&mut self, _route: &Route, _ctx: &Services) -> Box<dyn Page<Services>> {
        Box::new(Home)
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

## Core concepts

| Type | Responsibility |
| --- | --- |
| `Context` | Composition root for typed services, plus the route type and repaint signal |
| `Task<T, E>` | Cloneable observable task with `Idle`, `Loading`, `Ready`, `Error`, and `Cancelled` snapshots |
| `TaskCache<K, T, E>` | Keyed LRU task cache, 32 entries by default; eviction cancels in-flight work |
| `Page` | Route-local state, key handling, `poll`, and rendering |
| `PageEvent` / `NavAction` | A page's answer to a key, and `Push`, `Pop`, `Replace`, `Reset` navigation intents |
| `Navigator` | Route stack and `init`, `resume`, `pause`, `dispose` lifecycle |
| `ModalHost` / `ModalPage` | Callback-driven modal stack and zero-boilerplate page wrapper |
| `App` / `Redraw` | Event loop and the repaint signal shared by tasks and the loop |

## Async state

A `Task` stores the latest async result; clones share one snapshot. `Loading` and
`Error` retain the previous value, so `previous()` can keep stale data visible
while a request is running or has failed.

`TaskCache` caches requests by parameter, such as driver, account, or route ID.
It has an LRU capacity bound but no TTL; refresh data explicitly through methods
such as `reload()`.

Completion only triggers `Redraw`; it does not call the page directly. The page
reads snapshots and performs state transitions in `poll`:

```rust
fn poll(&mut self, ctx: &Services, _route: &Route) -> Option<NavAction<Route>> {
    if self.waiting && ctx.catalog.snapshot().value().is_some() {
        self.waiting = false;
        return Some(NavAction::Push(Route::Details));
    }
    None
}
```

`poll` is not a frame tick and is not called for key-driven changes. Keep it
cheap and idempotent. Only the active page is polled; paused pages should catch
up in `resume`.

## Navigation and modals

- A page returns `PageEvent::Navigate(NavAction::...)`; `Navigator` applies it and runs page lifecycle hooks.
- Removing a page with `Pop`, `Replace`, or `Reset` drops its local state; put retained state in a service.
- A shell page can own a nested `Navigator` to keep section routes independent.
- `ModalHost` stores modals plus completion callbacks; `ModalPage` forwards keys, rendering, hints, and `poll`.
- A modal completion callback can mutate the page or return a `NavAction`.

## Example

```bash
cargo run --example catalog
```

The example has two routes sharing one `Task`: `r` starts a request, `d` opens
details, `Backspace` goes back, and `Ctrl+Q` exits.

## Limitations

- Pages and rendering are synchronous; `Page::on_input` cannot await.
- `Event::Key`, `Event::Mouse` (when `App::enable_mouse` is called), and `Event::Paste` reach pages as normalized `InputEvent` values; focus events are not dispatched.
- A `Task` keeps the latest snapshot only; there are no progress streams, retries, backoff, or multi-consumer queues.
- `Task::new()` does not wake the UI; use `Task::with_redraw` when automatic repaints are needed.
- Routes live in an in-memory `Vec`; there is no persistence, deep-link restore, or parameter serialization.
