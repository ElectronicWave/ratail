# ratail

[English](README.md) | 简体中文

**轻量级 ratatui 异步框架。**

`ratail` 把路由内的状态放在页面里，把共享的应用状态放在带类型的 `Context` 里。输入处理是声明式的；异步任务的结果可以被观察：所有页面看到同一份快照，快照变化时 UI 才重绘。

```toml
[dependencies]
ratail = { path = "path/to/ratail" }
ratatui = "0.30"
tokio = { version = "1", features = ["macros", "rt", "time"] }
```

> [!NOTE]
> `ratail` 只跑在 tokio 上。页面是同步的，异步工作放在 `Context` 里的 `Task` 中。

## ratail 做了什么

- 每个页面自己管状态，不需要一个中央 app 结构体把所有屏幕的字段攒在一起。
- 共享的异步数据就是一个放进 `Context` 的 `Task`，每个页面读同一份快照。
- 快照变化时，任务通过 `Redraw` 唤醒事件循环，业务状态不用定时轮询。
- 页面直接返回 `PageEvent::Navigate`，导航不经过信道。
- `KeyMap` 在页面里把按键绑到 action 和提示；全局按键用闭包注册。
- 框架不内置退出键、action 枚举或帧间隔。

## 核心概念

| 类型 | 职责 |
| --- | --- |
| `Context` | 所有页面共享的带类型服务，以及路由类型和重绘信号 |
| `Task<T, E>` | 可克隆、可观察的异步任务，快照包含 `Idle`、`Loading`、`Ready`、`Error` 和 `Cancelled` |
| `TaskCache<K, T, E>` | 带键的任务缓存，共用同一个 `Redraw`，每个键维护独立快照 |
| `Page` | 路由内的状态、按键处理和渲染 |
| `PageEvent` | 页面对一次按键的回答：忽略、已处理，或导航意图 |
| `Navigator` | 路由栈和页面生命周期（`init`、`resume`、`pause`、`dispose`） |
| `App` | 事件循环、页面优先的按键路由、全局绑定和自动提示栏 |
| `Redraw` | 任务和应用循环共享的重绘信号 |

## 快速开始

下面的代码只画出核心协议。完整可运行的例子在 [`examples/catalog.rs`](examples/catalog.rs)。

### 服务与 Context

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

用同一个重绘信号创建共享任务：

```rust
let redraw = Redraw::new();
let catalog = Task::with_redraw(&redraw);
```

### 一个页面

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

    fn keybindings(&self) -> Vec<KeyHint> {
        self.keys.hints().to_vec()
    }

    fn render(&mut self, ctx: &Services, frame: &mut Frame, area: Rect) {
        draw_catalog(&ctx.catalog.snapshot(), frame, area);
    }
}
```

### 应用入口

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

### 自定义 hint 渲染

默认 footer 是一行按键与说明。需要不同布局或主题时可以替换渲染器：

```rust
use ratatui::style::{Color, Style};
use ratatui::widgets::{Paragraph, Widget};

app.set_hint_renderer(|_ctx, hints, area, buf| {
    Paragraph::new(ratail::hint_line(hints))
        .style(Style::default().fg(Color::Gray))
        .render(area, buf);
});
```

`hint_line` 会生成默认的 spans，自定义渲染器可以基于它重新着色，也可以完全绘制别的 widget。

退出键属于应用自己的策略，所以放在应用里。别的应用可以用 `Esc`、`q`，也可以不绑定退出键。

## 共享异步状态

`Task` 解决的是：多个地方调用同一个异步操作，读同一份结果。

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

- `Task` 克隆很便宜，可以放进服务里。
- 已经有任务在跑时，`spawn_once` 不会再启动一次。
- `Loading` 和 `Error` 会保留上一次的值，页面可以在报错时继续显示旧数据。
- `snapshot()` 返回当前状态，`value()`、`previous()` 和 `error()` 覆盖常见的读法。
- 身份基于 `Arc`，不需要分配任务 ID，也不用担心 ID 溢出。

### 带参数的请求

请求依赖某个参数时，比如 driver、账号或路由 ID，使用 `TaskCache`。每个键都相当于一个独立的 `Task`，不同参数的结果不会互相覆盖。它更像按 key 查询的缓存，而不是 React 的 `useEffect` 依赖列表：条目会一直保留，直到你主动移除。

```rust
struct Services {
    redraw: Redraw,
    catalogs: TaskCache<Driver, Catalog, Error>,
}

impl Services {
    fn catalog(&self, driver: Driver) -> Task<Catalog, Error> {
        self.catalogs.get_or_create(&driver)
    }

    fn load_catalog(&self, driver: Driver) -> bool {
        self.catalogs.spawn_once(driver, load_catalog)
    }
}

async fn load_catalog(driver: Driver) -> Result<Catalog, Error> {
    // 拉取这个 driver 的 catalog
}
```

`spawn_once` 接收 key，并把它传给请求函数。页面切换到另一个 key 时读取的是那个 key 的快照，不会覆盖之前的结果。

某个 key 不再需要时调用 `remove` 或 `clear`，两者都会取消进行中的请求。`snapshot` 对从未请求过的 key 返回 `None`，渲染路径不会偷偷写入缓存。

多个参数时，直接用元组或结构体作为 key：

```rust
#[derive(Clone, Eq, Hash, PartialEq)]
struct CatalogKey {
    driver: Driver,
    game_version: String,
}

let catalogs: TaskCache<CatalogKey, Catalog, Error> = TaskCache::new(&redraw);
```

读取快照仍然是显式的：

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
> 一个应用里共用同一个 `Redraw`。下一帧之前的多次触发会合并成一次重绘。

## 声明式输入与导航

绑按键时顺手把提示写在一起：

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

`App` 把页面提示和全局提示画在同一行页脚里，页面不用在每个屏幕里重复框架或应用的快捷键。

导航由页面返回：

```rust
PageEvent::Navigate(NavAction::Push(Route::Details))
PageEvent::Navigate(NavAction::Pop)
PageEvent::Navigate(NavAction::Replace(Route::Dashboard))
PageEvent::Navigate(NavAction::Reset(Route::Dashboard))
```

Navigator 会立刻应用这个动作：对旧页面执行 `pause`/`dispose`，再对新页面执行 `init`/`resume`。

## 与官方 async action flow 对比

官方材料里有两种异步写法：

- [`async-github`](https://github.com/ratatui/ratatui/tree/main/examples/apps/async-github) 示例把 widget 状态放在 `Arc<RwLock<_>>` 里，以固定 60 FPS 重绘。
- [官方 async 模板](https://github.com/ratatui/ratatui-website/tree/main/code/templates/components_async) 让所有东西都经过同一个无界信道：`mpsc::unbounded_channel::<Action>()`。输入、tick、resize 和后台结果都变成 `Action`，由应用在一个中央 `match` 里分发。见 [`app.rs`](https://github.com/ratatui/ratatui-website/blob/main/code/templates/components_async/src/app.rs) 和 [`action.rs`](https://github.com/ratatui/ratatui-website/blob/main/code/templates/components_async/src/action.rs)。

能扩展到多个组件的是 action-channel 模板，所以这里拿它来比。它的形态是：

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

每个生产者都要加一个 `Action` 变体，每个消费者都能看到整个枚举，分发归应用管。

`ratail` 里，共享数据是带类型的 `Task`，页面输出直接返回：

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

任务自己带着 `Loading`、`Ready`、`Error` 和 `Cancelled`，所有页面读同一份快照：

```rust
match &*ctx.catalog.snapshot() {
    Snapshot::Loading { previous } => {}
    Snapshot::Ready(catalog) => {}
    Snapshot::Error { error, previous } => {}
    _ => {}
}
```

| 关注点 | 官方 action-channel 流程 | ratail |
| --- | --- | --- |
| 消息传输 | 一个无界 `Action` 信道 | 带类型的 `Task` 句柄，加上直接返回的 `PageEvent` |
| 分发 | 应用里集中 `match action` | 页面内自己 match；没有应用级枚举 |
| 后台结果 | 生产者发 `Action::...` | Task 更新自己的快照 |
| 跨页数据 | 消息载荷和组件状态 | 把 `Task` 放进 `Context` |
| 加载/错误 | 手写状态变体 | `Snapshot` 自带，并保留上一次的值 |
| 重绘 | 固定 tick 或帧率 | `Redraw` 只在快照变化时唤醒循环 |
| 导航 | 再加一个 `Action` 变体 | `PageEvent::Navigate` |
| 输入 | 键位到 `Action`，再到信道 | `KeyMap` 到 `PageEvent`；全局键是闭包 |
| 提示 | 键位配置或手写字符串 | 框架把页面和全局提示合并到页脚 |
| 策略 | 模板定义 `Quit`、`Tick` 等 action | 不内置退出键、action 枚举或帧间隔 |

这个选择有代价。Action 信道是通用的 pub/sub 总线，能表达任意跨组件事件；`ratail` 把共享状态当成带类型的服务（`Task`），把页面输出当成意图（`PageEvent`）。你省掉了通用总线和中央分发器，代价是要自己实现 `Context`、`Page` 和 `Router`。

## 示例

catalog 例子跑通了整套流程：

```bash
cargo run --example catalog
```

它有两个路由，读同一个 `Task`。在任一页面按 `r` 只会发一次共享请求，两个页面显示同一个结果。第三次请求故意失败，用来看错误怎么渲染、旧数据怎么保留。`d` 进详情，`Backspace` 返回，`Ctrl+Q` 由应用注册。

## 限制

- `App::run` 是 async 的，跑在一个 tokio task 上；页面和渲染是同步的，`Page::on_key` 里不能 `await`。`Task` 会启动后台工作，所以服务类型必须满足 `Send + Sync`。
- `App` 把 `Event::Key` 交给页面。窗口缩放会触发重绘，但鼠标、粘贴和焦点事件不会送到页面。
- `Task` 只存一份快照，已有任务在跑时 `spawn_once` 直接跳过。没有进度流、重试、退避或多消费者队列。
- `Task::new()` 不会唤醒 UI，要用 `Task::with_redraw`。动画需要单独的定时任务调用 `Redraw::trigger`。
- Navigator 把路由存在 `Vec` 里，没有持久化、深链接恢复或路由参数序列化。
