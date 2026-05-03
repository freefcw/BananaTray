# Architecture

本文件只描述 BananaTray 的稳定架构边界。

如果某个结论依赖具体文件名、调用顺序或临时实现细节，请以当前代码和模块 `README.md` 为准，而不要把这里当作逐文件契约。

## Build Contract

- 默认受支持的产品路径是开启 `app` feature 的托盘应用构建。
- `bananatray` 二进制目标通过 Cargo `required-features = ["app"]` 显式要求该 feature。
- `app` 不只控制模块导出，也隔离托盘壳的运行时依赖（GPUI / adabraka-ui / 单实例 / 通知 / 自启动等）。
- `--no-default-features` 只保留给 `lib` 层的本地验证，不代表受支持的完整 app 构建模式；该模式下不应再引入 app-only 依赖。
- i18n 文案由 `rust-i18n` 从 `locales/*.yml` 编译进二进制；`build.rs` 必须跟踪 locale 文件变化，避免仅修改翻译后 Cargo 复用旧资源。

## Stable Module Boundaries

- `application/`
  - Action → Reducer → Effect 管线、纯状态变换、selector 组装。
  - 必须保持 GPUI-free。
- `models/`
  - Provider、Quota、Settings 等核心数据模型。
  - 必须保持 GPUI-free。
- `runtime/`
  - 共享前台状态、dispatcher、effect 执行、设置写入、设置窗口打开编排，以及全局热键解析、预检、注册/重绑。
  - `runtime/effects/` 按领域执行 GPUI-free 的 `CommonEffect`，避免把持久化、通知、refresh、Debug、NewAPI I/O 全部集中在 `runtime/mod.rs`。
  - macOS 的全局热键后端现使用系统级 `RegisterEventHotKey`，不再依赖 `NSEvent` monitor。
- `ui/`
  - GPUI 视图、窗口内容、控件和 view-local 状态。
- `theme.rs`
  - GPUI 主题 token、主题 YAML 解析和 `WindowAppearance` 到运行时主题的映射。
  - 仅在 `app` feature 下编译。
- `refresh/`
  - 后台刷新调度与并发执行。
- `providers/`
  - 内置 / 自定义 provider 实现、共享基础设施、ProviderManager。
- `platform/`
  - `paths` / `system` / 日志读取器等 lib-safe 平台能力。
  - `assets` / `single_instance` / `notification` / `auto_launch` 属于 app-only 平台适配层，只在 `app` feature 下编译。
- `tray/`
  - 托盘控制、弹窗生命周期、图标与定位。

## Shared State Model

前台共享状态由 `runtime::AppState` 持有。它是一个组合容器，而不是业务逻辑层本身。

稳定事实：

- `AppState` 持有：
  - `AppSession`
  - `ProviderManagerHandle`
  - refresh 请求通道
  - settings writer
  - 当前日志文件路径
- `AppSession` 持有：
  - `ProviderStore`
  - `NavigationState`
  - `SettingsUiState`
  - `DebugUiState`
  - `AppSettings`
  - quota alert tracker
  - popup 可见性状态

重要边界：

- `AppState` 不再保存 GPUI view 句柄。
- 具体视图对象和弱引用留在 `ui/`，只通过窄桥接接口与 `runtime/` 交互。

## Foreground Flow

前台主路径保持稳定为：

1. UI 交互或后台事件产生 `AppAction`
2. `runtime::dispatch_*()` 调用 reducer
3. reducer 返回 `Vec<AppEffect>`
4. runtime 执行 effect
5. 必要时请求 UI 重绘、打开窗口、发送 refresh 请求、预检并重绑全局热键，或写入设置

`AppEffect` 维持两类边界：

- `ContextEffect`
  - 需要 GPUI 前台上下文才能执行，例如重绘、开窗、应用 tray icon、重绑全局热键。
- `CommonEffect`
  - 不依赖具体 GPUI 上下文，例如持久化设置、发送 refresh 请求、普通 I/O。
  - 顶层按领域路由到 `SettingsEffect`、`NotificationEffect`、`RefreshEffect`、`DebugEffect`、`NewApiEffect`，由 `runtime/effects/` 下对应模块执行。

## Runtime / UI Ownership

稳定分工如下：

- `runtime/` 负责：
  - reducer 调用
  - effect 执行
  - 设置窗口打开 / 复用编排
  - 与 refresh / settings persistence 的对接
  - 为 Debug / Issue Report 收集平台信息、日志等诊断上下文
- `ui/` 负责：
  - popup 和 settings window 的具体视图类型
  - 渲染逻辑与 view-local state（例如设置页里的热键捕获控件）
  - 把少量必要 hook 注册给 `runtime/`

这意味着：

- `runtime/` 可以编排窗口行为，但不拥有视图实现。
- `ui/` 可以构造和刷新视图，但不承担全局副作用调度。

## Refresh Boundary

刷新系统的稳定约束：

- 后台刷新由独立的 `RefreshCoordinator` 执行。
- 调度决策由 `RefreshScheduler` 负责，核心规则包括：
  - 仅刷新已启用且 `ProviderCapability::Monitorable` 的 provider
  - 跳过 in-flight provider
  - 对 `Startup` / `Periodic` 应用 cooldown
  - `Manual` 和 `ProviderToggled` 可跳过 cooldown
- `Informational` / `Placeholder` provider 只保留展示入口，不进入启动、周期、手动、Debug 或 reload 后即时刷新链路。
- refresh 结果通过 `RefreshEvent` 回到前台，再进入 reducer。
- `RefreshRequest::UpdateConfig` 同步刷新调度配置和 app-managed provider credentials。后台协调器收到后会调用 `ProviderManager::sync_provider_credentials()`，让 Copilot 这类 token 面板 provider 的刷新线程读取到设置页保存的 override。

自定义 provider reload 的稳定语义：

- reload 会重建 provider manager 快照，并把最新状态发回前台。
- 当前没有“监视 providers 目录并自动 reload”的文件系统 watcher。
- 应用内的 NewAPI 保存 / 删除会显式触发 reload；手动编辑 YAML 后通常需要重启应用。

## Persistence And External Storage

`settings.json` 是用户偏好和 BananaTray 托管凭证的持久化入口。

- macOS: `~/Library/Application Support/BananaTray/settings.json`
- Linux: `$XDG_CONFIG_HOME/bananatray/settings.json`

自定义 provider YAML 的规范目录：

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `$XDG_CONFIG_HOME/bananatray/providers/`

稳定事实：

- 设置写入由后台 `settings_writer` 串行化并做 debounce。
- `settings.json` 使用原子替换写入。
- 外部 provider 的真实认证状态不一定存放在 `settings.json`，也可能来自环境变量、CLI 登录态或 provider 自己的文件。

## Localization Boundary

Provider 层和 refresh 层尽量只保存稳定语义，不缓存最终展示文案。

这带来两个稳定收益：

- 切换语言时无需强制刷新 provider 数据。
- 离线 / 缓存状态仍可在 selector 层重新格式化成当前语言。

## Workaround Register

下面这些 workaround 目前仍是有意保留的实现，不应在“顺手清理”时直接删掉：

| 位置 | 目的 | 触发条件 / 根因 | 删除条件 |
|------|------|-----------------|----------|
| `src/runtime/settings_window_opener.rs` 的 `10ms` 延迟打开 | 避免 tray/popup 关闭与 settings 建窗发生在同一轮前台事件处理里时出现窗口激活/生命周期时序问题 | 从 tray/popup 切到 settings 时，GPUI 窗口关闭和新窗口创建对同一轮事件循环较敏感；历史上出现过 `"window not found"` 类窗口时序问题 | 当 GPUI 或应用层能证明同轮关闭旧窗并立即建新窗稳定无回归，且多显示器/焦点切换路径实测通过 |
| `src/runtime/settings_window_opener.rs` 的 `+1px` resize nudge | 强制 settings window 在首次展示后重新走一次布局/绘制，避免初始尺寸或外观状态未完全刷新 | 新窗口刚激活时，GPUI 对首次 viewport/appearance 刷新存在时序敏感性 | 当去掉 nudge 后，多显示器、主题切换、冷启动开窗都能稳定保持正确布局和外观同步 |
| `src/bootstrap.rs` 在 macOS 启用 `set_tray_panel_mode(true)` | 保证点击菜单栏 status item 时进入 `on_tray_icon_event`，由应用打开 GPUI popup | GPUI macOS status item 默认是 NSMenu 模式；不启用 panel mode 时点击会走菜单路径而不是 tray icon callback，表现为点击托盘图标但弹窗不出现 | 当 GPUI macOS 默认点击行为改为稳定发出 tray icon callback，或应用改为用 NSMenu 作为 macOS 主交互入口 |
| `src/bootstrap.rs` 在 Linux 安装 tray menu（Open / Settings / Quit）作为 fallback | 为仍不稳定转发 `activate` / `secondary_activate` 的 tray host 保留可达入口，避免用户只能依赖左键点击 | 即使 tray callback bridge 已修复，不同 Wayland / Ubuntu tray host 对左键/次级激活的支持仍不一致；menu-based 入口是最后兜底，至少保证 Open / Settings / Quit 可用 | 当目标 Linux tray host 范围内已验证都会稳定发出 tray click 事件，且移除菜单 fallback 后 Ubuntu / Wayland / X11 实测仍可正常打开 popup / settings |
| `src/tray/controller.rs` 在 Linux 打开 popup 后显式 `show_window()`/`activate_window()`，且 auto-hide 只在 popup 至少成功激活过一次后才允许关闭 | 避免 Ubuntu / Linux 托盘点击后 popup 没被 WM/compositor 浮到前台，或在尚未真正获得焦点时被失焦观察器立即关掉，表现成“点击托盘没反应” | Linux 上 tray click 触发的建窗与焦点事件顺序不稳定，vendored GPUI 的 Linux `open_window` 也不消费 `WindowOptions.show/focus`，需要应用层补一次显式显示/激活，并把 auto-hide 收紧为“先激活过再允许失焦关闭” | 当 GPUI Linux 建窗对 tray-triggered popup 已能稳定映射并发出一致的激活状态变化，且移除这些保护后 Ubuntu / Wayland / X11 实测无回归 |
| Linux popup 复用窗口；拖动或已有保存位置后隐藏优先使用透明渲染 + 鼠标穿透，头部拖动时短暂抑制 auto-hide 并在抑制期后复查失焦，同时持久化 `settings.display.tray_popup.linux_last_position` | 让 Linux 用户在 Wayland 无法精确初始定位时仍可拖动 popup，并在同一进程内尽量保留窗口管理器放置结果；X11 下可跨重启恢复上次拖动位置 | Wayland `xdg_toplevel` 不允许客户端指定窗口位置，`hide_window()`/`show_window()` 可能重新映射到屏幕中央，且 `start_window_move()` 期间可能产生失焦事件；普通 `remove_window()`/重建会丢失 compositor 已放置的位置 | 当 GPUI Linux 支持 layer-shell / ext-layer-shell 等可控定位协议且可满足托盘弹窗交互，或确认所有目标桌面环境的普通窗口定位与拖动恢复稳定可控 |
| `src/platform/notification.rs` 中每条通知单独线程发送 | 避免通知发送路径阻塞或重入前台 GPUI 事件循环 | macOS 通知发送和系统事件回调可能与前台 UI 生命周期交错，历史上有 `RefCell` 重入风险 | 当通知发送链路被验证为可安全地在统一异步执行器/主线程桥接中运行，且不会引入重入或卡顿 |
| `src/refresh/coordinator.rs` 的 timeout guard 仅停止等待 | 保证单个卡死 provider 不会把整轮刷新和 in-flight 状态永久卡住 | Rust 线程池上的阻塞任务无法被协调器强制取消；CLI/HTTP 卡死时只能放弃等待结果 | 当底层刷新执行具备可传播的取消机制，或 provider 执行模型改成真正可中断的任务 |

## Testing Contract

- 标准测试命令是 `cargo test --lib`。
- `cargo test --lib --no-default-features` 应保持可用，用于验证 lib 层不会回流 app-only 依赖。
- `application/` 和 `models/` 是主要单元测试面。
- provider parser、scheduler、settings store、selector 也有独立测试。
- `runtime/` 和 `ui/` 仍属于 `app` feature 范围，但会尽量把纯逻辑抽离到可测试模块。

## What This Doc Does Not Promise

以下内容不再作为本文件的长期承诺：

- 完整文件树
- 逐函数调用链
- 精确测试数量
- 每个窗口或 provider 的内部文件布局

这些细节变化频率太高，继续写在这里只会制造新的文档漂移。
