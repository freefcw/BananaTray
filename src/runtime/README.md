# src/runtime/

前台运行时与 effect 执行层。负责持有共享运行时状态、调用 reducer、执行 effects，并协调 settings persistence / refresh / 热键 / 诊断等前台行为。需要窗口或 App 级能力的 shell 操作由 `bootstrap` 负责。

## Responsibilities

- 持有共享状态 `AppState`
- 提供 `dispatch_in_context()` 入口，把 `AppAction` 送入 reducer
- 执行 `AppEffect`，把声明式 effect 转成真实副作用
- 串行化设置持久化写入
- 解析、注册并热更新全局热键
- 收集 Debug / Issue Report 需要的诊断上下文
- 通过 `bootstrap` 的 shell facade 与具体 UI / tray / D-Bus 适配器交互

## Boundaries

- `application/` 只声明状态变化与 effects，不执行副作用
- `runtime/` 执行副作用，但不再把 UI 句柄存进 `AppState`
- `ui/` 持有具体 GPUI 视图类型，并提供 hooks factory；真正的注册动作由 `bootstrap` 统一完成

## Shell Boundary Decision

`runtime` 是前台内核，不是 shell owner。这里保留 reducer dispatch、effect routing、settings / refresh / diagnostics 等前台能力，以及 `ContextCapabilities` / `FullContextCapabilities` 这类能力抽象。

具体 shell 语义属于 `bootstrap`：settings window 的打开与复用、popup view hook registry、tray icon 应用、D-Bus handle 连接、App/Window 级 dispatch facade 都在那里组合。不要把旧的 `dispatch_in_app()` / `dispatch_in_window()`、`SettingsView` 句柄、D-Bus handle 或通用 shell manager 加回 `runtime`。如果新增 effect 需要强上下文能力，先扩展 capability trait 和 `bootstrap` adapter，而不是让 runtime 直接认识具体 UI / tray / D-Bus 类型。

## 两级路由架构

`AppEffect` 由两个子枚举组成：

| 子枚举 | 职责 | 新增时改动 |
|--------|------|-----------|
| `ContextEffect` | 需要 GPUI 上下文（Render, OpenSettingsWindow, OpenUrl, ApplyTrayIcon, ApplyGlobalHotkey, QuitApp） | `effect.rs` 定义 + `run_view_context_effect` / `run_full_context_effect`（必要时补 capability trait） |
| `CommonEffect` | GPUI-free 顶层领域路由（Settings / Notification / Refresh / Debug / NewApi / ScriptProvider） | 对应领域子枚举 + `runtime/effects/` 下同名执行器 |

`dispatch_in_context()` 使用两级路由。`CommonEffect` 委托给 `effects::run_common_effect` 做领域分派；`ContextEffect` 则由于各入口能力差异，使用 **Capability Trait** 模式进行收敛。`Window + App` / `App` 级别的完整 dispatch facade 已上移到 `bootstrap`。

`CommonEffect` 的具体变体按领域放在子枚举里：

| 子枚举 | 职责 |
|--------|------|
| `SettingsEffect` | 设置持久化、自启动同步、语言应用、日志级别应用 |
| `NotificationEffect` | quota 通知、普通文本通知、Debug 测试通知 |
| `RefreshEffect` | refresh 请求发送 |
| `DebugEffect` | Debug 页日志目录 / 剪贴板动作、日志捕获、Debug 刷新 |
| `NewApiEffect` | NewAPI 保存 / 删除 / 加载编排 |
| `ScriptProviderEffect` | 脚本 Provider 测试请求、保存脚本 + YAML、删除和编辑加载编排 |

### ContextCapabilities 模式

为了实现 `ContextEffect` 执行逻辑的收敛，同时避免错误 GPUI 上下文静默丢副作用，运行时把能力拆成 View-safe 与 Full context 两层；`bootstrap` 在 shell 层实现 Full context adapter：

- **`ContextCapabilities`** — View-safe 能力（Render, OpenUrl），可用于 `Context<V>` / `Window + App` / `App`。
- **`FullContextCapabilities`** — 强上下文能力（OpenSettingsWindow, ApplyTrayIcon, ApplyGlobalHotkey, QuitApp），只有 `Window + App` 与 `App` adapter 可实现。
- **`ViewCaps`** — `Context<V>` 的适配器，只执行 Render / OpenUrl；如果收到 OpenSettingsWindow / ApplyTrayIcon / ApplyGlobalHotkey / QuitApp，会立即 panic，暴露错误 dispatch 入口。
- `bootstrap` 额外实现两种 full-context adapter：

  - `WindowShellCaps` — `Window + App` 场景；负责窗口关闭、settings 复用、热键重绑和 tray 图标应用
  - `AppShellCaps` — `App` 场景；负责 popup 视图刷新、settings 打开调度、tray 图标应用和热键重绑

在这种模式下，新增一个 `ContextEffect` 变体需要先判断它是否 View-safe：View-safe effect 同步补 `run_view_context_effect()`；强上下文 effect 补 `FullContextCapabilities` 与 `run_full_context_effect()`。禁止用默认 `warn!` 降级吞掉不支持的强副作用。

### Dispatch 入口

| 函数 | GPUI 上下文 | 使用场景 |
|------|-------------|---------|
| `dispatch_in_context<V>()` | `Context<V>` | View render 回调中（如按钮点击） |
| `bootstrap::dispatch_in_window()` | `Window + App` | 窗口级事件处理（如设置窗口操作） |
| `bootstrap::dispatch_in_app()` | `App` | 全局事件（如后台刷新事件泵） |

所有 dispatch 函数共享同一流程：
1. 借用 `AppState`，调用 `reduce(&mut session, action)` 得到 `Vec<AppEffect>`
2. 释放借用
3. 将相应上下文包装进 Adapter
4. 逐个执行 effects (通过 context effect runner 或 `effects::run_common_effect`)

### 重入保护

`dispatch_effects()` 使用 `thread_local!` RAII guard 检测重入，防止 effect handler 中再次 dispatch 导致 `RefCell` 重入 panic。需要延迟分派的场景使用 `schedule_*` 系列函数。

## 子模块

### `app_state.rs` — 共享运行时状态容器

定义 `AppState`，作为 `runtime` 与 `ui` 共同使用的组合状态：

- `session: AppSession` — 纯状态树
- `manager: ProviderManagerHandle` — provider 运行时注册表共享句柄；UI 每次按需读取当前快照，后台 reload 时原子替换
- `refresh_tx` — 后台刷新请求通道
- `settings_writer: SettingsWriter` — 设置持久化串行写入器
- `log_path` — Debug 页展示的日志路径

`AppState` 已从 `ui` 模块迁出到 `runtime`，这样 `runtime` 不再依赖 `ui::AppState`，`ui` 改为消费 `runtime::AppState`。弹窗视图弱引用与设置窗口构造入口现在注册到 `bootstrap` 的 shell hook registry，避免把 UI 句柄直接存进 `AppState`。

`ProviderManagerHandle` 的引入是为了消除 reload 后的前后台分叉：`RefreshCoordinator` 和设置页 token 面板都通过同一个句柄拿快照，自定义 provider 热重载时只替换内部 `Arc<ProviderManager>`，不再各自保留旧 manager。设置页保存的 app-managed provider credentials 会随 `RefreshRequest::UpdateConfig` 发给后台协调器，再由 `ProviderManager::sync_provider_credentials()` 注入需要运行时凭证快照的 provider。

### `bootstrap.rs` + `bootstrap/` — Shell Hook Registry

`bootstrap` 承担 shell composition root 的职责：`bootstrap.rs` 是薄模块入口，`bootstrap/`
按生命周期边界拆分 UI / settings window / worker bridge / event source / D-Bus 的具体适配器入口：

- 请求当前 popup view 重新渲染
- 清理 popup view 注册
- 构造 settings window 的 view entity
- 调度 settings window 打开与复用
- 实现 `Window + App` / `App` 级 full-context dispatch facade
- 通过 `bootstrap/event_sources/` 注册 tray / hotkey / secondary-instance 事件源
- 通过 `bootstrap/workers/` 桥接 refresh 与 script provider test 后台 worker 到前台 reducer
- 在 Linux 上把 `DBusServiceHandle` 作为 event pump 的局部输入，用于更新快照缓存并发射信号

`ui/` 只负责提供 hooks factory；`bootstrap` 统一注册这些 hooks，`runtime` 只调用抽象端口，不再知道 `SettingsView` 或 `DBusServiceHandle` 的具体归属。

### `gpu_cache.rs` — GPUI Resource Trimming

`gpu_cache.rs` 在启动阶段注册全局 `on_window_closed` observer。窗口关闭后会 debounce 一个短延迟；若此时没有 GPUI 窗口存活，则调用 `App::trim_gpu_caches()`。这是上游 GPUI 的 best-effort GPU pool 回收接口，用于释放 popup / settings window 关闭后空闲的 renderer buffer，同时避免 popup 切 settings window 时刚释放又重建；Linux popup 的隐藏复用路径不关闭窗口，因此不会触发这条回收。

### `settings_writer.rs` — 设置文件 Debounce 写入器

合并短时间内的多次 `PersistSettings` 请求，避免快速操作（拖拽排序、连续切换）时频繁写盘。

- **`SettingsWriter::spawn()`** — 启动后台写入线程，返回句柄（存放在 `AppState` 上）
- **`schedule(settings)`** — 异步 debounce 写入，500ms 窗口内合并多次调用，只写最后一份
- **`flush(settings)`** — 同步写入，立即落盘并返回结果，会打断未落盘的 debounce 窗口
- 所有写入（schedule 和 flush）通过同一个后台线程串行化，避免乱序覆盖

### `diagnostics_context.rs` — 诊断上下文收集

封装 Debug Tab 和 Issue Report 所需的运行时数据读取，包括日志文件元数据、日志捕获缓冲区、系统信息、locale、当前日志级别和构建信息。`application/selectors` 只接收已收集好的 `DebugContext` / `IssueReportContext` 并保持纯函数边界。

### `effects/` — CommonEffect 领域执行器

封装所有不依赖 GPUI 上下文的 effect handler，避免 `runtime/mod.rs` 成为副作用中心化增长点：

- `mod.rs` — `CommonEffect` 顶层穷尽分派
- `settings.rs` — `SettingsEffect`
- `notification.rs` — `NotificationEffect`
- `refresh.rs` — `RefreshEffect`，并提供共享的 refresh 请求发送 helper
- `debug.rs` — `DebugEffect`
- `newapi.rs` — `NewApiEffect`
- `script_provider.rs` — `ScriptProviderEffect`

各子模块只暴露 `run()` 或少量同领域 helper。NewAPI 与脚本 Provider 的 YAML / 脚本、编辑态加载、删除都统一放在 `providers::custom::api`，纯状态回滚仍在 `application/newapi_ops.rs` / `application/script_provider_ops.rs`。脚本 Run Test 通过独立事件泵在后台线程执行，结果再回到前台 reducer，避免阻塞设置窗口。

### `global_hotkey.rs` — 全局热键解析与重绑

封装 `system.global_hotkey` 的字符串解析、格式归一化、冲突预检、运行时重新注册和失败回滚：

- **`parse_hotkey_string()` / `format_hotkey_for_settings()`** — 兼容旧版展示格式输入（如 `Cmd+S`），但持久化统一写成可回读格式（如 `cmd-s`），避免单字符 key round-trip 时被误加 `Shift`
- **`register_hotkey_string()`** — 正式替换前先用 probe id 做一次预检；若 probe 失败则返回冲突错误且保持当前热键不动，若正式替换失败则尽力恢复旧热键
- **`rebind_global_hotkey()`** — settings save 路径的入口：成功时更新 `AppSettings` 并同步写盘，失败时把错误回填到 `SettingsUiState.global_hotkey_error`
- 启动阶段也复用同一套规则；若磁盘配置无效，`bootstrap` 会先把配置修正为默认热键再尝试注册，因此即便默认热键本身也注册失败，磁盘里也不会继续残留不可解析的坏值；若配置合法但注册失败，则保留用户原值并回填错误
- macOS 底层注册现改为系统级 `RegisterEventHotKey`，并使用 exclusive 选项注册，避免继续依赖 `NSEvent` monitor 的监听式实现；Windows / X11 仍沿用各自平台 API

### `providers::custom::api` — Custom Provider lifecycle API

封装 `NewApiEffect::SaveProvider` / `DeleteProvider` 以及 `ScriptProviderEffect::SaveProvider` / `DeleteProvider` / `LoadConfig` 需要的稳定入口：

- **`generate_filename()` / `generate_script_yaml_filename()` / `generate_script_filename()`** — 由 custom provider id / config 推导落盘文件名
- **`default_script_template()`** — Settings 窗口脚本 provider 新增页的默认模板
- **`read_newapi_config()` / `read_script_provider_config()`** — 按 YAML `id` 读取编辑态回填数据
- **`save_newapi_yaml(config, filename) → Result<PathBuf, String>`** — YAML 生成 + 目录创建 + 文件写入
- **`delete_newapi_yaml(provider_id) → Result<PathBuf, String>`** — 校验 NewAPI provider id + 复用 `providers/custom/locator.rs` 按 YAML `id` 定位真实文件 + 删除 YAML 文件
- **`save_script_provider(config, yaml_filename, script_filename) → Result<(PathBuf, PathBuf), String>`** — 写入脚本文件，再生成 `source.type: cli` YAML
- **`delete_script_provider_files(provider_id) → Result<(PathBuf, Result<PathBuf, String>), String>`** — 校验 `{slug}:script` provider id；YAML 删除成功即移除 provider，companion script 删除失败会作为 partial 结果上报

回滚逻辑位于 `application/newapi_ops.rs` / `application/script_provider_ops.rs`（纯函数，可测试）；runtime 在删除失败时负责记录日志并发送用户通知。

## 约束

- 本模块在 `cfg(feature = "app")` 下编译，依赖 GPUI
- Effect handler 中**不得**调用 `dispatch_*()` — 使用 `schedule_*` 延迟到下一轮事件循环
- 通知线程切换统一由 `platform::notification` 负责，runtime 只触发通知 effect，避免重复 `spawn`

## Data Flow

```text
AppAction
  -> runtime::dispatch_in_context() / bootstrap::dispatch_in_app() / bootstrap::dispatch_in_window()
  -> application::reduce(&mut AppState.session, action)
  -> Vec<AppEffect>
  -> run_context_effect / effects::run_common_effect
  -> GPUI / tray / refresh / settings_store / providers side effects
```
