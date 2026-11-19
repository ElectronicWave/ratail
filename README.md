# ratail

English | [简体中文](README.zh.md)

**Lightweight async framework for ratatui.**

`ratail` keeps route-local state in pages and shared application state in a typed `Context`. Input handling is declarative, and async task results are observable: every page sees the same snapshot, and the UI repaints when that snapshot changes.

```toml
[dependencies]
ratail = { path = "path/to/ratail" }
ratatui = "0.30"
tokio = { version = "1", features = ["macros", "rt", "time"] }
```

> [!NOTE]
> `ratail` only runs on tokio. Pages are synchronous; async work lives in `Task`s stored in the context.

## What ratail does

- Pages own their state. There is no central app struct collecting every screen's fields.
- Shared async data lives in a domain service inside the `Context`. Every page reads the same snapshot.
- Tasks wake the event loop through `Redraw` when their snapshot changes, so business state needs no polling interval.
- A page returns `PageEvent::Navigate` to move; navigation does not go through a channel.
- Page-local `KeyMap`s carry actions and hints. Global keys are registered as closures.
- The framework has no built-in quit key, action enum, or frame interval.

## Core concepts

| Type | Responsibility |
| --- | --- |
| `Context` | Composition root for typed domain services, plus the route type and repaint signal |
| `Task<T, E>` | Cloneable observable async task with `Idle`, `Loading`, `Ready`, `Error`, and `Cancelled` snapshots |
| `TaskCache<K, T, E>` | Keyed task cache sharing one `Redraw`; each key keeps an independent snapshot |
| `Page` | Route-local state, key handling, and rendering |
| `PageEvent` | The page's answer to a key: ignored, handled, or a navigation intent |
| `Navigator` | Route stack and page lifecycle (`init`, `resume`, `pause`, `dispose`) |
| `App` | Event loop, global-first key routing, page dispatch, and automatic hint footer |
| `Redraw` | Repaint signal shared by tasks and the application loop |

## Quick start

The snippets below sketch the core protocols. A complete, runnable app lives in [`examples/catalog.rs`](examples/catalog.rs).

### Services and context

```rust
struct Services {
    ui: UiService,
    catalog: CatalogService,
}

impl Services {
    fn catalog(&self) -> &CatalogService {
        &self.catalog
    }
}

impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        self.ui.redraw()
    }
}
```

For a multi-domain app, treat `Services` as the composition root. Keep its fields
private, expose domain services through accessors, and let each service own its
own tasks:

```rust
struct UiService {
    redraw: Redraw,
}

impl UiService {
    fn redraw(&self) -> &Redraw {
        &self.redraw
    }
}

struct CatalogService {
    catalog: Task<Catalog, String>,
}

impl CatalogService {
    fn new(redraw: &Redraw) -> Self {
        Self {
            catalog: Task::with_redraw(redraw),
        }
    }

    fn snapshot(&self) -> Arc<Snapshot<Catalog, String>> {
        self.catalog.snapshot()
    }

    fn reload(&self) {
        self.catalog.spawn_once(load_catalog);
    }
}
```

Pass the same `Redraw` to every service constructor. Pages call service methods
such as `ctx.catalog().reload()`; they do not touch `spawn_once` or construct
tasks directly. A small app with one or two tasks can keep a flat `Services`.

### A page

```rust
impl Page<Services> for Dashboard {
    fn on_key(&mut self, ctx: &Services, _route: &Route, key: KeyEvent) -> PageEvent<Route> {
        match self.keys.action_for(&key) {
            Some(Action::Reload) => {
                ctx.catalog().reload();
                PageEvent::Handled
            }
            Some(Action::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
            None => PageEvent::Ignored,
        }
    }

    fn keybindings(&self, _route: &Route) -> Vec<KeyHint> {
        self.keys.hints().to_vec()
    }

    fn render(&mut self, ctx: &Services, _route: &Route, frame: &mut Frame, area: Rect) {
        draw_catalog(&ctx.catalog().snapshot(), frame, area);
    }
}
```

### The application

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let mut app = App::new(Services::new(), AppRouter, Route::Dashboard);
    app.bind_global(
        KeyChord::ctrl('q'),
        KeyHint::new("Ctrl+Q", "quit"),
        |_| AppFlow::Exit,
    );

    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal).await;
    ratatui::restore();
    result
}
```

## Shared async state

A `Task` is for calling the same async operation from several places and reading
one result. Keep it in the domain service that owns the request.

- Cloning a `Task` is cheap enough to store it in a service.
- `spawn_once` does nothing while a run is already in flight.
- `Loading` and `Error` keep the previous value, so a page can show stale data next to an error.
- `snapshot()` returns the current state. `value()`, `previous()`, and `error()` cover the usual reads; `view()` flattens status, data, and error when stale data and an error must be rendered together.
- Identity is `Arc`-based, so there are no task IDs to allocate or overflow.

### Parameterized requests

When the request depends on a parameter, such as a driver, account, or route id, use `TaskCache`. Every key behaves like a separate `Task`, so results for different parameters never overwrite each other. Think of it as a keyed query cache, not as React's `useEffect` dependency list: entries are retained until you remove them.

```rust
struct CatalogService {
    catalogs: TaskCache<Driver, Catalog, Error>,
}

impl CatalogService {
    fn catalog(&self, driver: Driver) -> Task<Catalog, Error> {
        self.catalogs.get_or_create(&driver)
    }

    fn load_catalog(&self, driver: Driver) -> bool {
        self.catalogs.spawn_once(driver, load_catalog)
    }
}

async fn load_catalog(driver: Driver) -> Result<Catalog, Error> {
    // fetch the catalog for this driver
}
```

`spawn_once` takes the key and passes it to the request function. Switching a page to another key reads that key's snapshot instead of replacing the previous result.

Call `remove` or `clear` when a key is no longer needed; both cancel in-flight requests. `snapshot` returns `None` for keys that were never requested, so rendering stays free of hidden cache writes. For several parameters, use a tuple or a struct as the key.

Reading a snapshot stays explicit:

```rust
match &*ctx.catalog().snapshot() {
    Snapshot::Idle => {}
    Snapshot::Loading { previous } => {}
    Snapshot::Ready(catalog) => {}
    Snapshot::Error { error, previous } => {}
    Snapshot::Cancelled { previous } => {}
}
```

When a page renders stale data and errors together, flatten the snapshot first. `view()` keeps the error visible even when `data` comes from the previous result:

```rust
let snapshot = ctx.catalog().snapshot();
let view = snapshot.view();

if let Some(catalog) = view.data {
    draw_catalog(catalog, frame, area);
}
if let Some(error) = view.error {
    draw_error(error, frame, area);
}
```

> [!TIP]
> Share one `Redraw` between all tasks in an application. Triggers that happen before the next frame collapse into one repaint.

## Declarative input and navigation

Bind each key next to its hint:

```rust
keys.bind_with_hint(
    KeyChord::char('r'),
    Action::Reload,
    KeyHint::new("r", "reload"),
);
keys.bind_with_hint(
    KeyChord::char('d'),
    Action::Details,
    KeyHint::new("d", "details"),
);
```

`App` draws page hints and global hints on one footer line, so pages do not repeat framework or application shortcuts on every screen.
Use `App::set_hint_renderer` to replace the footer layout or styling; `hint_line` exposes the default spans.

Global bindings are reserved chords. `App` resolves them before dispatching to the
page, so a page that returns `PageEvent::Handled` cannot shadow a global shortcut.
If a page needs a chord, do not register it globally.

Navigation is returned from the page: `PageEvent::Navigate` takes `NavAction::Push`,
`Pop`, `Replace`, or `Reset`. The navigator applies the action right away: it
pauses and disposes the old page, then initializes and resumes the new one.

## Compared with the official async action flow

The [`async-github`](https://github.com/ratatui/ratatui/tree/main/examples/apps/async-github)
example keeps widget state in `Arc<RwLock<_>>` and repaints at a fixed 60 FPS.
The [async template](https://github.com/ratatui/ratatui-website/tree/main/code/templates/components_async)
routes input, ticks, and background results through one
`mpsc::unbounded_channel::<Action>()` and a central `match`.

`ratail` replaces that bus with typed handles and direct page output: shared
async data is a `Task` owned by a service in `Context`, pages return `PageEvent`,
and `Redraw` wakes the loop only when a snapshot changes.

| Concern | Official action-channel flow | ratail |
| --- | --- | --- |
| Message transport | One unbounded `Action` channel | Typed `Task` handles plus direct `PageEvent` returns |
| Dispatch | Central `match action` in the app | Page-local action match; no app-wide enum |
| Background result | Producer sends `Action::...` | Task updates its own snapshot |
| Cross-page data | Payloads and component state | Store the `Task` in `Context` |
| Loading/error | Hand-written state variants | Built into `Snapshot`, with the previous value retained |
| Repaint | Tick/frame rate | `Redraw` wakes the loop only when a snapshot changes |
| Navigation | Another `Action` variant | `PageEvent::Navigate` |
| Input | Keybinding, then `Action`, then channel | `KeyMap`, then `PageEvent`; global keys are closures |
| Hints | Keybinding config or manual strings | Framework merges page and global hints into a footer |
| Policy | Template defines `Quit`, `Tick`, and other actions | No built-in quit key, action enum, or frame interval |

The trade-off is deliberate: an action channel is a general pub/sub bus, while
`ratail` treats shared state as a typed service and page output as an intent.
You lose the general bus and central dispatcher, and implement `Context`,
`Page`, and `Router` yourself.

## Example

The catalog example runs the whole flow:

```bash
cargo run --example catalog
```

It has two routes reading the same `Task`. Pressing `r` on either page starts one shared request, and both pages render the same result. The third request fails on purpose so you can see error rendering with the previous data still on screen. `d` opens details, `Backspace` goes back, and `Ctrl+Q` is registered by the app.

## Limitations

- `App::run` is async and lives on a single tokio task. Pages and rendering are synchronous, so `Page::on_key` cannot await. Service types must be `Send + Sync` because `Task` spawns work.
- `App` sends `Event::Key` to pages. Resize triggers a repaint, but mouse, paste, and focus events never reach a page.
- A `Task` stores one snapshot and `spawn_once` skips while a run is in flight. There are no progress streams, retries, backoff, or multi-consumer queues.
- `Task::new()` does not wake the UI; use `Task::with_redraw`. Animations need their own timer task calling `Redraw::trigger`.
- The navigator keeps routes in a `Vec`. Nothing is persisted, deep links are not restored, and route parameters are not serialized.
