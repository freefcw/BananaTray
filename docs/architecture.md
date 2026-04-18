# Architecture

本文件只描述 BananaTray 的稳定架构边界。

如果某个结论依赖具体文件名、调用顺序或临时实现细节，请以当前代码和模块 `README.md` 为准，而不要把这里当作逐文件契约。

## Build Contract

- 默认受支持的产品路径是开启 `app` feature 的托盘应用构建。
- `bananatray` 二进制目标通过 Cargo `required-features = ["app"]` 显式要求该 feature。
- `--no-default-features` 只保留给 `lib` 层的本地验证，不代表受支持的完整 app 构建模式。

## Stable Module Boundaries

- `application/`
  - Action → Reducer → Effect 管线、纯状态变换、selector 组装。
  - 必须保持 GPUI-free。
- `models/`
  - Provider、Quota、Settings 等核心数据模型。
  - 必须保持 GPUI-free。
- `runtime/`
  - 共享前台状态、dispatcher、effect 执行、设置写入、设置窗口打开编排。
- `ui/`
  - GPUI 视图、窗口内容、控件和 view-local 状态。
- `refresh/`
  - 后台刷新调度与并发执行。
- `providers/`
  - 内置 / 自定义 provider 实现、共享基础设施、ProviderManager。
- `platform/`
  - 通知、路径、自启动、日志、系统交互等平台能力。
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
5. 必要时请求 UI 重绘、打开窗口、发送 refresh 请求或写入设置

`AppEffect` 维持两类边界：

- `ContextEffect`
  - 需要 GPUI 前台上下文才能执行，例如重绘、开窗、应用 tray icon。
- `CommonEffect`
  - 不依赖具体 GPUI 上下文，例如持久化设置、发送 refresh 请求、普通 I/O。

## Runtime / UI Ownership

稳定分工如下：

- `runtime/` 负责：
  - reducer 调用
  - effect 执行
  - 设置窗口打开 / 复用编排
  - 与 refresh / settings persistence 的对接
- `ui/` 负责：
  - popup 和 settings window 的具体视图类型
  - 渲染逻辑与 view-local state
  - 把少量必要 hook 注册给 `runtime/`

这意味着：

- `runtime/` 可以编排窗口行为，但不拥有视图实现。
- `ui/` 可以构造和刷新视图，但不承担全局副作用调度。

## Refresh Boundary

刷新系统的稳定约束：

- 后台刷新由独立的 `RefreshCoordinator` 执行。
- 调度决策由 `RefreshScheduler` 负责，核心规则包括：
  - 仅刷新已启用 provider
  - 跳过 in-flight provider
  - 对 `Startup` / `Periodic` 应用 cooldown
  - `Manual` 和 `ProviderToggled` 可跳过 cooldown
- refresh 结果通过 `RefreshEvent` 回到前台，再进入 reducer。

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

## Testing Contract

- 标准测试命令是 `cargo test --lib`。
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
