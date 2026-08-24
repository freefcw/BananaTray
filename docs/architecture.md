# Architecture

本文件只描述 BananaTray 的稳定架构边界。

如果某个结论依赖具体文件名、调用顺序或临时实现细节，请以当前代码和模块 `README.md` 为准，而不要把这里当作逐文件契约。

## Build Contract

- 默认受支持的产品路径是开启 `app` feature 的托盘应用构建。
- `bananatray` 二进制目标通过 Cargo `required-features = ["app"]` 显式要求该 feature。
- `app` 不只控制模块导出，也隔离托盘壳的运行时依赖（GPUI / adabraka-ui / 单实例 / 通知 / 自启动等）。
- GPUI 启动使用 `AppProfile::Minimal`，让托盘类长驻进程采用较小的文本布局缓存、glyph raster-bounds 缓存、GPU atlas / instance buffer 初始预算和 element arena。
- `adabraka-ui` 以 `default-features = false` 引入时不会自动注册内置字体；BananaTray 在 `Cargo.toml` 显式开启 Inter 400/500/600/700 和 JetBrains Mono Regular，保留当前 UI 字重覆盖，同时避免嵌入未使用的 Mono Bold。
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
  - 共享前台状态、dispatcher、effect 执行、设置写入，以及全局热键解析、预检、注册/重绑。
  - `runtime/effects/` 按领域执行 GPUI-free 的 `CommonEffect`，避免把持久化、通知、refresh、Debug、NewAPI / 脚本 Provider I/O 全部集中在 `runtime/mod.rs`。
  - macOS 的全局热键后端现使用系统级 `RegisterEventHotKey`，不再依赖 `NSEvent` monitor。
- `bootstrap.rs` + `bootstrap/`
  - shell composition root；`bootstrap.rs` 是薄入口，`bootstrap/` 按职责拆分 full-context dispatch facade、popup/settings hook registry、settings window 生命周期、UI 启动。
  - `bootstrap/workers/` 承担 refresh/script-test worker 到前台 reducer 的 bridge，以及 Linux D-Bus 快照发射。
  - `bootstrap/event_sources/` 承担 app shutdown、tray events、startup hotkey、secondary instance bridge 的外部事件源注册。
  - 统一注册 UI hooks，并持有具体 tray / settings window / D-Bus 适配器入口；不引入 runtime-owned shell manager。
- `ui/`
  - GPUI 视图、窗口内容、控件和 view-local 状态，以及向 `bootstrap` 提供 hooks factory。
- `theme/`
  - GPUI 主题 token、主题 YAML 解析和 `WindowAppearance` 到运行时主题的映射。
  - 仅在 `app` feature 下编译。
- `refresh/`
  - 后台刷新调度与并发执行。
- `providers/`
  - 内置 / 自定义 provider 实现、共享基础设施、ProviderManager。
- `dbus/`
  - D-Bus 服务，供 GNOME Shell Extension 查询配额数据。仅 Linux + `app` feature 下编译。
  - 对外接口：`DBusServiceHandle`（更新缓存 + 发射信号）+ DTO 类型（re-export 自 `application::selectors::dbus_dto`）。
  - Linux deb/rpm 安装包提供 `com.bananatray.Daemon` 的 Session D-Bus activation 文件和 `bananatray.service` systemd user unit；Extension 启动和用户主动操作时会异步请求 activation。AppImage 不提供宿主 D-Bus activation。
  - 线程模型：2 线程（D-Bus 线程运行 zbus ObjectServer，GPUI 主线程通过 foreground executor 消费 action）。
  - `BananaTrayIface` 不持有 `AppState`（zbus `Interface` 要求 `Send + Sync`，`Rc<RefCell<_>>` 不满足），改用 `Arc<Mutex<String>>` 快照缓存 + channel 通信。
  - DTO 类型和格式化函数定义在 `application::selectors::dbus_dto`（跨平台可测试），`dbus/serde_types.rs` 仅做 re-export。
- `platform/`
  - `paths` / `system` / 日志读取器等 lib-safe 平台能力。
  - `assets` / `single_instance` / `notification` / `auto_launch` 属于 app-only 平台适配层，只在 `app` feature 下编译。
  - `gnome_detect.rs` — GNOME 桌面 + BananaTray 扩展检测（Linux only，需扩展已启用且 `gnome-extensions info` 显示 `State: ACTIVE`）。
  - GNOME nested 调试脚本会设置 `BANANATRAY_FORCE_GNOME_EXTENSION=1` 和 `BANANATRAY_SINGLE_INSTANCE_SUFFIX=gnome-dev`，让同一 nested D-Bus session 中的真实 app 只服务扩展、且不与主会话实例冲突；这两个环境变量仅用于开发调试。
- `tray/`
  - 托盘弹窗生命周期（controller）、入口命令策略（command）、失焦状态机（activation）、observer 注册、定位策略（positioning）、Linux popup 行为、图标管理。

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

- `AppState` 不再保存 GPUI view 句柄或 D-Bus 句柄。
- 具体视图对象和弱引用留在 `ui/`，只通过窄桥接接口与 `bootstrap` / `runtime` 交互。

## Foreground Flow

前台主路径保持稳定为：

1. UI 交互或后台事件产生 `AppAction`
2. `runtime::dispatch_in_context()` 或 `bootstrap::dispatch_in_app()` / `bootstrap::dispatch_in_window()` 调用 reducer
3. reducer 通过单个穷尽 match 将 action 分派到领域函数并返回 `Vec<AppEffect>`；不使用家族二次 match 的 `unreachable!` 兜底
4. runtime 执行 effect
5. 必要时请求 UI 重绘、打开窗口、发送 refresh 请求、预检并重绑全局热键，或写入设置

`AppEffect` 维持两类边界：

- `ContextEffect`
  - 需要 GPUI 前台上下文才能执行，例如重绘、开窗、应用 tray icon、重绑全局热键。
- `CommonEffect`
  - 不依赖具体 GPUI 上下文，例如持久化设置、发送 refresh 请求、普通 I/O。
  - 顶层按领域路由到 `SettingsEffect`、`NotificationEffect`、`RefreshEffect`、`DebugEffect`、`NewApiEffect`、`ScriptProviderEffect`，由 `runtime/effects/` 下对应模块执行。

## Runtime / Bootstrap / UI Ownership

稳定分工如下：

- `runtime/` 负责：
  - reducer 调用
  - effect 执行
  - 与 refresh / settings persistence 的对接
  - 为 Debug / Issue Report 收集平台信息、日志等诊断上下文
- `bootstrap/` 负责：
  - full-context dispatch facade
  - popup view 注册与清理
  - settings window 打开 / 复用编排
  - tray 图标应用
  - tray / hotkey / secondary-instance 事件源注册
  - refresh 与 script-test 后台 worker 到前台 reducer 的 bridge
  - Linux D-Bus 事件泵连接
- `ui/` 负责：
  - popup 和 settings window 的具体视图类型
  - 渲染逻辑与 view-local state（例如设置页里的热键捕获控件）
  - 提供给 `bootstrap` 注册的 hooks factory

这意味着：

- `runtime/` 保持内核职责，不直接依赖具体 UI / tray / D-Bus 类型。
- `bootstrap/` 作为组合根连接具体适配器。
- `ui/` 可以构造和刷新视图，但不承担全局副作用调度。

### Shell Boundary Decision

当前边界的设计结论是：**runtime 是前台内核，bootstrap 是 shell composition root**。

`runtime/` 只需要 reducer、effect 执行和 capability abstraction。它不需要、也不应该拥有“shell 边界服务”。需要具体窗口、托盘、D-Bus、退出、重开或 App/Window 上下文的动作，由 `bootstrap/` 的 full-context adapter 组合后调用 `runtime::dispatch_with_full_context()`。

禁止回流的模式：

- 不要把 `dispatch_in_app()` / `dispatch_in_window()` 重新放回 `runtime/`
- 不要把 `SettingsView`、settings window handle、popup weak ref 或 `DBusServiceHandle` 存进 `runtime::AppState`
- 不要为了兼容旧入口新增 runtime-owned shell helper / shell manager / bridge service
- 不要让 `ui/` 或 `dbus/` 直接请求具体 shell 语义；它们应发 `AppAction`，由 `bootstrap` 承担 shell 组合

如果未来需求迫使 `runtime/` 认识具体窗口类型、具体 tray 实现或 D-Bus handle，应先重审这条边界，而不是局部补一个兼容入口。

## Refresh Boundary

刷新系统的稳定约束：

- 后台刷新由独立的 `RefreshCoordinator` 执行。
- 调度决策由 `RefreshScheduler` 负责，核心规则包括：
  - 仅刷新已启用且 `ProviderCapability::Monitorable` 的 provider
  - 跳过 in-flight provider
  - 对 `Startup` / `Periodic` 应用 cooldown
  - `Manual` 和 `ProviderToggled` 可跳过 cooldown
- `Informational` / `Placeholder` provider 只保留展示入口，不进入启动、周期、手动、Debug 或 reload 后即时刷新链路。
- refresh 结果通过 `RefreshEvent` 回到前台，再进入 reducer。主循环不等待 Provider I/O，活跃刷新期间仍可处理配置、reload 和 shutdown。
- 同一 Provider 始终保持 single-flight：timeout 只结束前台等待，底层阻塞任务真实完成前不会释放执行占用。
- `RefreshRequest::UpdateConfig` 同步刷新调度配置和 app-managed provider credentials。凭证、启用列表或 registry 变化会推进 generation；旧 generation 的迟到结果不会更新 quota 或触发通知。后台执行时仍通过 `ProviderExecutionContext` 显式传递当前凭证快照。

自定义 provider reload 的稳定语义：

- YAML 运行时契约为 `schema_version: 2` + `plan.steps`；加载旧 YAML 时会自动迁移并写回，详见 `custom-provider.md`。
- reload 会重建 provider manager 快照，并把最新状态发回前台。
- 当前没有文件系统 watcher；触发规则和 reload 语义详见 `refresh-strategy.md` §Custom Provider Reload。

## Persistence And External Storage

`settings.json` 是用户偏好和 BananaTray 托管凭证的持久化入口。

- macOS: `~/Library/Application Support/BananaTray/settings.json`
- Linux: `$XDG_CONFIG_HOME/bananatray/settings.json`

自定义 provider YAML 与脚本向导生成脚本的规范目录见 `custom-provider.md` §配置目录。

稳定事实：

- 设置写入由后台 `settings_writer` 串行化并做 debounce；正常应用退出会关闭 writer sender、执行 pending snapshot 的 final flush，并 join 后台线程。随后，退出钩子把内存中的最终 `start_at_login` 状态提交给同一个 auto-launch worker，等待应用完成后再结束进程。
- `settings.json`、BananaTray 代写的外部 OAuth 凭证和自定义 provider YAML 复用私有文件写入原语：同目录临时文件、Unix `0600`、写入同步后 rename，并在可恢复失败路径清理临时文件。脚本 provider 使用不可变版本化脚本，最后原子提交引用它的 YAML；成功后再清理旧脚本，避免崩溃窗口产生跨版本文件对。
- `settings.json` 加载失败（JSON 损坏等）时，启动路径会先把原文件 rename 备份为 `settings.json.corrupt-<epoch>` 再回退默认值，避免后续 persist 覆盖后原始内容不可恢复；备份成功时启动后发送系统通知告知备份位置。
- 外部 provider 的真实认证状态不一定存放在 `settings.json`，也可能来自环境变量、CLI 登录态或 provider 自己的文件。

## Localization Boundary

Provider 层和 refresh 层尽量只保存稳定语义，不缓存最终展示文案。

这带来两个稳定收益：

- 切换语言时无需强制刷新 provider 数据。
- 离线 / 缓存状态仍可在 selector 层重新格式化成当前语言。

## Workaround Register

下面这些 workaround 目前仍是有意保留的实现，不应在“顺手清理”时直接删掉。

**优先级说明**：P1 = 直接影响用户体验；P2 = 防御性，可能已不需要；P3 = 平台/语言限制，短期不会变。

**最近核查日期**：2026-06-18

| 位置 | 目的 | 触发条件 / 根因 | 删除条件 | 创建日期 | 上游追踪 | 优先级 |
|------|------|-----------------|----------|---------|---------|--------|
| `src/bootstrap/settings_window.rs` 的 `10ms` 延迟打开 | 避免 tray/popup 关闭与 settings 建窗发生在同一轮前台事件处理里时出现窗口激活/生命周期时序问题 | 从 tray/popup 切到 settings 时，GPUI 窗口关闭和新窗口创建对同一轮事件循环较敏感；历史上出现过 `"window not found"` 类窗口时序问题 | 当 GPUI 或应用层能证明同轮关闭旧窗并立即建新窗稳定无回归，且多显示器/焦点切换路径实测通过 | 2026-04 | adabraka-gpui `open_window` 时序 | P2 |
| `src/bootstrap/settings_window.rs` 的 `+1px` resize nudge | 强制 settings window 在首次展示后重新走一次布局/绘制，避免初始尺寸或外观状态未完全刷新 | 新窗口刚激活时，GPUI 对首次 viewport/appearance 刷新存在时序敏感性 | 当去掉 nudge 后，多显示器、主题切换、冷启动开窗都能稳定保持正确布局和外观同步 | 2026-04 | adabraka-gpui viewport 刷新 | P2 |
| `src/platform/popup_window.rs` 的 macOS 顶边固定 `setFrame` | 打开弹窗或切 tab 时改高度，钉住顶边、关掉 AppKit 动画 | GPUI `Window::resize()` 在 macOS 异步 `setContentSize:`，原点在左下角；PopUp 还开了 UtilityWindow 动画 | 当上游 `resize()` 改为可关动画并保持顶边（或提供 `set_bounds`）且切 tab 实测不再跳 | 2026-08 | adabraka-gpui macOS `resize` → `setContentSize` | P1 |
| `AppView` 在 Overview 停留期间不随展开/折叠改窗口高度 | 展开/折叠只改卡片，不触发原生窗口 resize，避免整窗抖动 | GPUI PopUp 改 `contentSize` / drawable 无法做到无闪的实时长高 | 当 GPUI 支持无闪的同步 `set_bounds` 且展开跟随高度实测稳定 | 2026-08 | adabraka-gpui macOS PopUp resize | P1 |
| `src/bootstrap/ui_bootstrap.rs` 在 macOS 启用 `set_tray_panel_mode(true)` | 保证点击菜单栏 status item 时进入 `on_tray_icon_event`，由应用打开 GPUI popup | GPUI macOS status item 默认是 NSMenu 模式；不启用 panel mode 时点击会走菜单路径而不是 tray icon callback，表现为点击托盘图标但弹窗不出现 | 当 GPUI macOS 默认点击行为改为稳定发出 tray icon callback，或应用改为用 NSMenu 作为 macOS 主交互入口 | 2026-03 | adabraka-gpui macOS tray | P1 |
| `src/bootstrap/ui_bootstrap.rs` 注册 `on_window_closed` 后延迟调用 `trim_gpu_caches()` | 最后一个 GPUI 窗口关闭后释放 renderer 中闲置的 pooled GPU buffer，降低托盘应用长期后台驻留的 GPU 内存占用 | 上游 GPUI 的 trim 是 best-effort 且只回收 idle renderer pool；关闭 popup / settings window 后短暂延迟并确认没有窗口，避免 popup 切 settings 时刚释放又重建 | 当 GPUI 自身在窗口关闭后自动回收这些 idle pool，或应用不再长驻后台 | 2026-05 | adabraka-gpui renderer pool | P2 |
| `src/bootstrap/event_sources/tray.rs` 在 Linux 安装 tray menu（Open / Settings / Quit）作为 fallback | 为仍不稳定转发 `activate` / `secondary_activate` 的 tray host 保留可达入口，避免用户只能依赖左键点击 | 即使 tray callback bridge 已修复，不同 Wayland / Ubuntu tray host 对左键/次级激活的支持仍不一致；menu-based 入口是最后兜底，至少保证 Open / Settings / Quit 可用 | 当目标 Linux tray host 范围内已验证都会稳定发出 tray click 事件，且移除菜单 fallback 后 Ubuntu / Wayland / X11 实测仍可正常打开 popup / settings | 2026-04 | adabraka-gpui Linux KSNI + tray host 差异 | P1 |
| `src/tray/controller.rs` + `src/tray/activation.rs`：Linux 打开 popup 后显式 `show_window()`/`activate_window()`（via `linux_popup::ensure_popup_visible`），activation 状态机只在 popup 至少成功激活过一次后才允许关闭 | 避免 Ubuntu / Linux 托盘点击后 popup 没被 WM/compositor 浮到前台，或在尚未真正获得焦点时被失焦观察器立即关掉，表现成“点击托盘没反应” | Linux 上 tray click 触发的建窗与焦点事件顺序不稳定，vendored GPUI 的 Linux `open_window` 也不消费 `WindowOptions.show/focus`，需要应用层补一次显式显示/激活，并把 auto-hide 收紧为“先激活过再允许失焦关闭” | 当 GPUI Linux 建窗对 tray-triggered popup 已能稳定映射并发出一致的激活状态变化，且移除这些保护后 Ubuntu / Wayland / X11 实测无回归 | 2026-05 | adabraka-gpui Linux `open_window` + WM 焦点时序 | P1 |
| Linux popup 复用窗口；拖动或已有保存位置后隐藏优先使用透明渲染 + 鼠标穿透，头部拖动时短暂抑制 auto-hide 并在抑制期后复查失焦，同时持久化 `settings.display.tray_popup.linux_last_position` | 让 Linux 用户在 Wayland 无法精确初始定位时仍可拖动 popup，并在同一进程内尽量保留窗口管理器放置结果；X11 下可跨重启恢复上次拖动位置 | Wayland `xdg_toplevel` 不允许客户端指定窗口位置，`hide_window()`/`show_window()` 可能重新映射到屏幕中央，且 `start_window_move()` 期间可能产生失焦事件；普通 `remove_window()`/重建会丢失 compositor 已放置的位置 | 当 GPUI Linux 支持 layer-shell / ext-layer-shell 等可控定位协议且可满足托盘弹窗交互，或确认所有目标桌面环境的普通窗口定位与拖动恢复稳定可控 | 2026-05 | adabraka-gpui Linux 窗口定位 + Wayland 协议 | P3 |
| `src/platform/notification.rs` 中每条通知单独线程发送 | 避免通知发送路径阻塞或重入前台 GPUI 事件循环 | macOS 通知发送和系统事件回调可能与前台 UI 生命周期交错，历史上有 `RefCell` 重入风险 | 当通知发送链路被验证为可安全地在统一异步执行器/主线程桥接中运行，且不会引入重入或卡顿 | 2026-04 | macOS `UNUserNotificationCenter` 回调时序 | P3 |
| `src/refresh/coordinator.rs` 的 timeout guard 只报告前台超时，不能取消底层任务 | 让 UI 及时结束等待，同时继续持有 per-provider single-flight，防止同一 Provider 重叠执行 | Rust 线程池上的阻塞任务无法被协调器强制取消；CLI/HTTP 卡死时只能忽略其迟到结果并等待真实结束后释放 lease | 当底层刷新执行具备可传播的取消机制，或 provider 执行模型改成真正可中断的任务 | 2026-04 | Rust std 线程不可取消 | P3 |

## Testing Contract

- 标准测试命令是 `cargo test --lib`。
- `cargo test --lib --no-default-features` 应保持可用，用于验证 lib 层不会回流 app-only 依赖。
- 主路径 CI 使用 `cargo clippy --lib --no-default-features -- -D warnings` 和 `cargo test --lib --no-default-features` 作为快速门禁；完整默认 feature clippy、`cargo test --lib` 与 `cargo check --bin bananatray` 在 Rust/依赖/主题相关 PR、App CI 手动触发和定时检查中运行。
- Provider secret/token 预览必须复用 `providers::common::secret::mask_secret_preview`；`scripts/check-provider-secret-slicing.sh` 在 CI / pre-commit 中禁止 `src/providers` 重新出现直接字节切片式预览。
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
