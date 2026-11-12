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
- Shared async data is a `Task` you put in the `Context`. Every page reads the same snapshot.
- Tasks wake the event loop through `Redraw` when their snapshot changes, so business state needs no polling interval.
- A page returns `PageEvent::Navigate` to move; navigation does not go through a channel.
- Page-local `KeyMap`s carry actions and hints. Global keys are registered as closures.
- The framework has no built-in quit key, action enum, or frame interval.

## Core concepts

| Type | Responsibility |
| --- | --- |
| `Context` | Typed services shared by every page, plus the route type and repaint signal |
| `Task<T, E>` | Cloneable observable async task with `Idle`, `Loading`, `Ready`, `Error`, and `Cancelled` snapshots |
| `Page` | Route-local state, key handling, and rendering |
| `PageEvent` | The page's answer to a key: ignored, handled, or a navigation intent |
| `Navigator` | Route stack and page lifecycle (`init`, `resume`, `pause`, `dispose`) |
| `App` | Event loop, page-first key routing, global binds, and automatic hint footer |
| `Redraw` | Repaint signal shared by tasks and the application loop |

## Quick start

The snippets below sketch the core protocols. A complete, runnable app lives in [`examples/catalog.rs`](examples/catalog.rs).

### Services and context

```rust
struct Services {
    redraw: Redraw,
    catalog: Task<Catalog, String>,
}

impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        &self.redraw
    }
}
```

Create the shared task with the same repaint signal:

```rust
let redraw = Redraw::new();
let catalog = Task::with_redraw(&redraw);
```

### A page

```rust
impl Page<Services> for Dashboard {
    fn on_key(&mut self, ctx: &Services, key: KeyEvent) -> PageEvent<Route> {
        match self.keys.action_for(&key) {
            Some(Action::Reload) => {
                ctx.catalog.spawn_once(load_catalog);
                PageEvent::Handled
            }
            Some(Action::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
            None => PageEvent::Ignored,
        }
    }

    fn keybindings(&self) -> &[KeyHint] {
        self.keys.hints()
    }

    fn render(&mut self, ctx: &Services, frame: &mut Frame, area: Rect) {
        draw_catalog(&ctx.catalog.snapshot(), frame, area);
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

The quit key is application policy, so it sits in the application. Another app can use `Esc`, `q`, or no quit key at all.

## Shared async state

A `Task` is for calling the same async operation from several places and reading one result.

```rust
impl Services {
    fn reload(&self) {
        if self.catalog.is_running() {
            return;
        }

        self.catalog.spawn_once(load_catalog);
    }
}
```

- Cloning a `Task` is cheap enough to store it in a service.
- `spawn_once` does nothing while a run is already in flight.
- `Loading` and `Error` keep the previous value, so a page can show stale data next to an error.
- `snapshot()` returns the current state. `value()`, `previous()`, and `error()` cover the usual reads.
- Identity is `Arc`-based, so there are no task IDs to allocate or overflow.

Reading a snapshot stays explicit:

```rust
match &*ctx.catalog.snapshot() {
    Snapshot::Idle => {}
    Snapshot::Loading { previous } => {}
    Snapshot::Ready(catalog) => {}
    Snapshot::Error { error, previous } => {}
    Snapshot::Cancelled { previous } => {}
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

Navigation is returned from the page:

```rust
PageEvent::Navigate(NavAction::Push(Route::Details))
PageEvent::Navigate(NavAction::Pop)
PageEvent::Navigate(NavAction::Replace(Route::Dashboard))
PageEvent::Navigate(NavAction::Reset(Route::Dashboard))
```

The navigator applies the action right away. It pauses and disposes the old page, then initializes and resumes the new one.

## Compared with the official async action flow

The official material contains two async shapes:

- The [`async-github`](https://github.com/ratatui/ratatui/tree/main/examples/apps/async-github) example keeps widget state in `Arc<RwLock<_>>` and repaints on a fixed 60 FPS tick.
- The [async template](https://github.com/ratatui/ratatui-website/tree/main/code/templates/components_async) routes everything through one unbounded channel: `mpsc::unbounded_channel::<Action>()`. Input, ticks, resizes, and background results all become `Action`s, and the app dispatches them in one central `match`. See [`app.rs`](https://github.com/ratatui/ratatui-website/blob/main/code/templates/components_async/src/app.rs) and [`action.rs`](https://github.com/ratatui/ratatui-website/blob/main/code/templates/components_async/src/action.rs).

The action-channel template is the one that scales to multiple components, so it is the one compared here. Its shape is:

```rust
let (action_tx, action_rx) = mpsc::unbounded_channel::<Action>();

tokio::spawn(async move {
    let catalog = load_catalog().await;
    action_tx.send(Action::CatalogLoaded(catalog))?;
});

while let Ok(action) = action_rx.try_recv() {
    match action {
        Action::Reload => {}
        Action::CatalogLoaded(catalog) => {}
        Action::Quit => break,
        _ => {}
    }
}
```

Every producer needs an `Action` variant, every consumer sees the whole enum, and the app owns the dispatcher.

With `ratail`, shared data is a typed `Task` and pages return their own output:

```rust
struct Services {
    catalog: Task<Catalog, String>,
}

fn on_key(&mut self, ctx: &Services, key: KeyEvent) -> PageEvent<Route> {
    match self.keys.action_for(&key) {
        Some(Action::Reload) => {
            ctx.catalog.spawn_once(load_catalog);
            PageEvent::Handled
        }
        Some(Action::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
        None => PageEvent::Ignored,
    }
}
```

The task carries `Loading`, `Ready`, `Error`, and `Cancelled` itself, and every page reads the same snapshot:

```rust
match &*ctx.catalog.snapshot() {
    Snapshot::Loading { previous } => {}
    Snapshot::Ready(catalog) => {}
    Snapshot::Error { error, previous } => {}
    _ => {}
}
```

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

This choice has a cost. An action channel is a general pub/sub bus and can express arbitrary cross-component events. `ratail` treats shared state as a typed service (`Task`) and page output as an intent (`PageEvent`). You lose the general bus and the central dispatcher, and you have to implement `Context`, `Page`, and `Router` yourself.

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
