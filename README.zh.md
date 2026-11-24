# ratail

[English](README.md) | 简体中文

轻量级 ratatui 异步 UI 框架。页面同步、状态分片、异步结果可观察；没有全局 action 总线和固定帧率。

## 安装

```toml
[dependencies]
ratail = { path = "path/to/ratail" }
ratatui = "0.30"
tokio = { version = "1", features = ["macros", "rt", "time"] }
```

> [!NOTE]
> 只支持 tokio。页面是同步的，异步工作放在 `Context` 里的 `Task` 中。

## 架构

```text
terminal input ─┐
                ├─> App::run ─> Navigator ─> current Page
Redraw ─────────┘                 └─ on_input / poll / render

Task completes -> update snapshot -> Redraw.trigger()
```

`App::run` 只有两个唤醒源：终端事件和 `Redraw`。归一化后的按键、body 区域内的鼠标事件（通过 `App::enable_mouse` 开启捕获）以及粘贴事件统一交给当前页面的 `on_input`；异步状态变化交给 `poll`。每次 redraw 最多 poll 当前页面一次，并最多应用一个 `NavAction`。

状态放在哪里：

- **页面本地状态**：只属于当前路由实例，页面被移除后消失。
- **`Context` 服务**：跨页面共享，页面通过只读的 `&C` 和领域服务方法访问。
- **`Task` / `TaskCache`**：异步结果；`render` 读快照，`poll` 做一次性转换。

整体是**分片式 MVU + immediate-mode 渲染**：每个页面负责自己的状态转换，没有中央 model、全局 action 或虚拟 DOM。

## 快速开始

以下片段省略了部分 import；完整示例见 [`examples/catalog.rs`](examples/catalog.rs)。

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

## 核心概念

| 类型 | 职责 |
| --- | --- |
| `Context` | 带类型领域服务的组合根，以及路由类型和重绘信号 |
| `Task<T, E>` | 可克隆、可观察的异步任务；快照包含 `Idle`、`Loading`、`Ready`、`Error`、`Cancelled` |
| `TaskCache<K, T, E>` | 带 key 的 LRU 任务缓存，默认 32 条；淘汰时取消进行中的请求 |
| `Page` | 路由局部状态、按键处理、`poll` 和渲染 |
| `PageEvent` / `NavAction` | 页面对按键的回答，以及 `Push`、`Pop`、`Replace`、`Reset` 导航意图 |
| `Navigator` | 路由栈和 `init`、`resume`、`pause`、`dispose` 生命周期 |
| `ModalHost` / `ModalPage` | 回调驱动的弹窗栈和零样板页面包装器 |
| `App` / `Redraw` | 事件循环，以及任务和循环共享的重绘信号 |

## 异步状态

`Task` 保存最新一次异步结果，多个 clone 共享同一份快照。`Loading` 和 `Error` 会保留上一次的值，可以用 `previous()` 在加载或报错时继续显示旧数据。

`TaskCache` 适合按参数缓存请求，例如 driver、账号或路由 ID。它只有 LRU 容量限制，没有 TTL；数据刷新由业务显式调用 `reload()` 之类的方法。

任务完成只触发 `Redraw`，不会直接回调页面。页面在 `poll` 中读取快照并完成状态转换：

```rust
fn poll(&mut self, ctx: &Services, _route: &Route) -> Option<NavAction<Route>> {
    if self.waiting && ctx.catalog.snapshot().value().is_some() {
        self.waiting = false;
        return Some(NavAction::Push(Route::Details));
    }
    None
}
```

`poll` 不是帧 tick，也不会因为按键变化而调用；它必须便宜且幂等。只有当前页面会被 poll，暂停的页面应在 `resume` 时补状态。

## 导航与弹窗

- 页面返回 `PageEvent::Navigate(NavAction::...)`，`Navigator` 立即应用并执行页面生命周期。
- 页面被 `Pop`、`Replace` 或 `Reset` 移除后，本地状态会消失；需要保留的状态放进服务。
- Shell 页面可以持有嵌套 `Navigator`，把路由按区域拆开。
- `ModalHost` 保存弹窗及其完成回调；`ModalPage` 自动转发按键、渲染、提示和 `poll`。
- 弹窗完成回调可以直接修改页面，也可以返回 `NavAction` 继续导航。

## 示例

```bash
cargo run --example catalog
```

示例有两个路由，共享同一个 `Task`；`r` 发起请求，`d` 进入详情，`Backspace` 返回，`Ctrl+Q` 退出。

## 限制

- 页面和渲染是同步的，`Page::on_input` 不能 `await`。
- `Event::Key`、`Event::Mouse`（通过 `App::enable_mouse` 开启终端鼠标捕获）与 `Event::Paste` 会归一化为 `InputEvent` 分发给页面；焦点事件不会分发。
- `Task` 只保留最新快照，没有进度流、重试、退避或多消费者队列。
- `Task::new()` 不会唤醒 UI；需要自动重绘时使用 `Task::with_redraw`。
- 路由只存在内存中的 `Vec` 里，没有持久化、深链接恢复或参数序列化。
