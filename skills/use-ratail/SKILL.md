---
name: use-ratail
description: Use this skill whenever you build or change a ratatui terminal app that depends on ratail, including adding a page, wiring a route, sharing async data through Task, handling keys, or debugging lifecycle and repaint behavior. Trigger it when Cargo.toml contains ratail, when the user mentions Page, Context, Task, NavAction, KeyMap, or PageEvent in a ratatui project, or when they ask to convert a ratatui app to ratail. Do not use it for plain ratatui apps that do not depend on ratail.
---

# Use ratail

Ratail is an async ratatui kernel. Pages are synchronous state machines, shared services live in a typed `Context`, and async work lives in cloneable `Task<T, E>` values. There is no global action channel or central dispatcher by default.

## Inspect the project first

Before writing code, check the local version of ratail:

- Read `Cargo.toml` for the dependency path or version.
- If the project uses a local path, read `src/lib.rs` and the relevant modules. If it uses a published version, read the installed crate source when the API is not obvious.
- Find the existing route type, shared services, global keys, and any footer or layout assumptions.

The API is small, but guessing from a different version is the fastest way to produce code that does not compile.

## Wiring order

1. Define the route type and implement `Context` on the services struct.
2. Implement `Router<C>` so each route produces a boxed page.
3. Implement `Page<C>` for each page.
4. Build `App::new(context, router, root)` and run it inside a tokio runtime.
5. Register application-wide keys with `App::bind_global`.

For a new app or a large rewrite, read [references/app-skeleton.md](references/app-skeleton.md) and adapt it. The skeleton is intentionally complete so the pieces line up, but keep the project's existing module layout and naming.

## Context and routes

```rust
impl Context for Services {
    type Route = Route;

    fn redraw(&self) -> &Redraw {
        &self.redraw
    }
}
```

`Context` requires `Sized + Send + Sync + 'static`. `Route` requires `Clone + Send + Sync + 'static`. Put cloneable services and shared tasks in the context. `Router::create` receives `&C`, so page construction can read shared state.

## Shared async state

Create tasks that should repaint with `Task::with_redraw(&ctx.redraw)`. `Task::new()` works, but it does not wake the event loop when a run finishes.

- `spawn_once` starts a future only when no run is in flight and returns whether it started. It takes `FnOnce() -> Future<Output = Result<T, E>>`.
- `Snapshot` is `Idle`, `Loading { previous }`, `Ready(Arc<T>)`, `Error { error, previous }`, or `Cancelled { previous }`.
- `Loading` and `Error` retain the previous value. Read it with `snapshot.previous()` and render stale data next to the error.
- `value()` returns data only for `Ready`; `error()` returns an error only for `Error`.
- `cancel()` records `Cancelled`; `reset()` returns `false` while a run is active.
- A task keeps one latest result. Progress streams, retry queues, and multiple consumers belong in a service, not in `Task`.

Store a task in the context when several pages need the same result. A page-local task is also valid if it has a `Redraw`.

```rust
match &*ctx.catalog.snapshot() {
    Snapshot::Idle => {}
    Snapshot::Loading { previous } => {}
    Snapshot::Ready(catalog) => {}
    Snapshot::Error { error, previous } => {}
    Snapshot::Cancelled { previous } => {}
}
```

## Pages, keys, and actions

`Action` is not a ratail type. Define a small `Clone + Copy` enum per page or focus scope, or match `KeyEvent` directly. `KeyMap<A>::action_for` requires `A: Copy`.

```rust
match self.keys.action_for(&key) {
    Some(HomeAction::Reload) => {
        ctx.reload();
        PageEvent::Handled
    }
    Some(HomeAction::Details) => PageEvent::Navigate(NavAction::Push(Route::Details)),
    None => PageEvent::Ignored,
}
```

- `PageEvent::Ignored` lets the app try global bindings.
- `PageEvent::Handled` stops global handling.
- `PageEvent::Navigate` applies navigation and stops global handling.
- `KeyMap::bind` panics on a duplicate chord. Release events are ignored; press and repeat both resolve.
- Render hints with `keybindings()`. `App` merges page hints and global hints into one footer line. With no hints, the page gets the full frame area.

Global keys return `AppFlow`:

```rust
app.bind_global(KeyChord::ctrl('q'), KeyHint::new("Ctrl+Q", "quit"), |_| {
    AppFlow::Exit
});
```

Keep actions local to the page that handles them. Building one app-wide action enum and a central `match` recreates the pattern ratail is meant to replace.

## Navigation and lifecycle

`NavAction` has `Push(route)`, `Pop`, `Replace(route)`, and `Reset(route)`.

- `Navigator::new` creates the root page, then calls `init` and `resume`.
- `Push` pauses the current page, creates the new page, then calls `init` and `resume`.
- `Pop` does nothing at the root. Otherwise it pauses and disposes the removed page, then resumes the new top page.
- `Replace` pauses and disposes the current page, then creates the new page and calls `init` and `resume`.
- `Reset` disposes every page, then creates the new root and calls `init` and `resume`.

`init` runs on every page creation, not once per route type. Put repeatable setup there. Use `resume` for work that should run whenever a page becomes visible again.

## Runtime behavior

- `App::run` draws first, then waits for a terminal event or `Redraw::requested()`.
- Only `Event::Key` is routed to pages. Resize causes another draw, but mouse, paste, and focus events do not reach pages.
- Pages and rendering are synchronous, so `Page::on_key` cannot await. Spawn work through a `Task`.
- For animation or timers, run a task that calls `Redraw::trigger`. There is no configured frame interval.

## Common mistakes

- Using `Task::new()` when automatic repaints are expected.
- Calling `spawn_once` outside a tokio runtime.
- Assuming `Pop` can leave the root, or that `init` runs only once.
- Putting large non-`Copy` payloads in a `KeyMap` action enum.
- Repeating global shortcuts in every page's hints; `App` already merges them.
- Adding a global `Action` channel instead of using `Task` and `PageEvent`.
