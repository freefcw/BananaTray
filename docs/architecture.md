# Architecture

本文件只描述 BananaTray 的稳定架构边界。

如果某个结论依赖具体文件名、调用顺序或临时实现细节，请以当前代码和模块 `README.md` 为准，而不要把这里当作逐文件契约。

## Build Contract

- 默认受支持的产品路径是开启 `app` feature 的托盘应用构建。
- `bananatray` 二进制目标通过 Cargo `required-features = ["app"]` 显式要求该 feature。
- `src/main.rs` 是只调用 `bananatray::run_app()` 的薄入口；唯一模块图和完整启动流程位于 lib crate，避免 bin/lib 各编译一份同名模块。
- `app` 不只控制模块导出，也隔离托盘壳的运行时依赖（GPUI / fc-ui / 单实例 / 通知 / 自启动 / Linux zbus 等）。
- GPUI 与 UI 组件库都来自 crates.io 正式发布版：`fc-gpui` 0.11.2（Cargo 依赖键仍是 `gpui`，`use gpui::*` 不变）和 `fc-ui` 0.9.2（crate root 即 `fc_ui`，无包别名）。`fc-ui` 自身依赖 `fc-gpui ^0.11.2`，两边必须保持同一 minor 对齐，否则 GPUI 会被解析成两份直接编译失败。
- 这两个包要求 rustc ≥ 1.90（`fc-ui` 声明 `rust-version = 1.90`，`fc-gpui` 使用 edition 2024）；项目通过 `rust-toolchain.toml` 固定 Rust 1.98.1 及 rustfmt/clippy 组件，避免 stable 滚动升级导致 CI 的 lint 行为未经评估就变化。1.98.1 同时是 GitHub runner 镜像（ubuntu 24.04 与 26.04、macOS）预装的 Rust 版本，CI 冷跑不需要 rustup 下载 toolchain。workflow 里的 `dtolnay/rust-toolchain` 统一引用 `@v1` tag 并显式传 `toolchain` 输入——上游已删除按 Rust 版本编号的 tag，写 `@1.98.1` 这类版本引用会 resolve 失败。
- GPUI 启动使用 `AppProfile::Minimal`，让托盘类长驻进程采用较小的文本布局缓存、glyph raster-bounds 缓存、GPU atlas / instance buffer 初始预算和 element arena。
- 托盘常驻依赖 `QuitMode::Explicit`（`fc-gpui` 0.9 取代旧的 `set_keep_alive_without_windows`）：所有窗口关闭后进程不退出，只有显式 quit 才结束。
- `fc-ui` 以 `default-features = false` 引入时不会自动注册内置字体；BananaTray 在 `Cargo.toml` 显式开启 Inter 400/500/600/700 和 JetBrains Mono Regular，保留当前 UI 字重覆盖，同时避免嵌入未使用的 Mono Bold。
- `--no-default-features` 只保留给 `lib` 层的本地验证，不代表受支持的完整 app 构建模式；该模式下不应再引入 app-only 依赖，Linux target 的依赖树也不得包含 `zbus`。
- i18n 文案由 `rust-i18n` 从 `locales/*.yml` 编译进二进制；`build.rs` 必须跟踪 locale 文件变化，避免仅修改翻译后 Cargo 复用旧资源。各 locale 文件的 key 集合（嵌套展平后）与 `%{placeholder}` 占位符集合必须互相对齐，缺 key 只在运行时暴露；`scripts/check_locales.py`（`just check-locales`，已并入 `ci-fast`，且由 GitHub CI 直接执行）负责该校验。

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
  - `bootstrap/workers/` 承担 refresh/custom-provider CRUD/script-test worker 到前台 reducer 的 bridge，以及 Linux D-Bus 快照发射。
  - `bootstrap/event_sources/` 承担 app shutdown、tray events、startup hotkey、secondary instance bridge 的外部事件源注册。
  - 统一注册 UI hooks，并持有具体 tray / settings window / D-Bus 适配器入口；不引入 runtime-owned shell manager。
- `ui/`
  - GPUI 视图、窗口内容、控件和 view-local 状态，以及向 `bootstrap` 提供 hooks factory。
- `theme/`
  - GPUI 主题 token、主题 YAML 解析和 `WindowAppearance` 到运行时主题的映射。
  - 仅在 `app` feature 下编译。
- `timing.rs`
  - app-only 的跨层生命周期时序策略，集中维护退出、设置写入和设置窗口打开协议使用的命名时长。
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
  - `AlertEngine`（阈值状态与用量步长提醒的领域状态）
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
  - refresh、custom-provider CRUD 与 script-test 后台 worker 到前台 reducer 的 bridge
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
- 配额通知是前台契约：只有成功且 enabled 的刷新结果在前台 reducer 中构造统一 `QuotaObservation` 并交给 `AlertEngine`，告警 effect 在同一 dispatch 内发出；后台 worker 不做告警决策。`AlertEngine` 组合独立的阈值状态策略和用量步长策略，共享同一份 effective rules / quota 快照，但分别维护状态。告警覆盖四类——LowQuota / Exhausted / Recovered / UsageProgress（按全局或 Provider 级步长跟踪最差剩余百分比的累计消耗，一次下降只发一条，回升或旧告警发生时重建基线不补发）。前三类按各 quota 的原生剩余值与 `QuotaRules` 中对应单位的 `notify` 阈值判定（`remaining <= notify` → Low，`remaining <= 0` → Exhausted，inclusive `<=`）：Credit 用货币余额，Points 和非 Credit 纯余额用原生额度，其余用剩余百分比。颜色档（warning / critical）与 Low 判定共用包含原始计算尺度的浮点边界容差，耗尽仍严格 `remaining <= 0`；完整精度契约见 `src/models/quota/README.md`。状态事件优先于同一次刷新中的用量事件。规则全局默认百分比 10 / 货币 1 / 积分 10（notify 档，颜色档见 `models::quota::policy`），Provider 可按单位整组覆盖；保存阈值设置仅当某 Provider 当前有效额度单位的 `notify` 阈值实际变化时才重建该 Provider 的告警档位。用量步长只采样 `limit > 0` quota 的最差剩余百分比，纯余额（balance-only）quota 不参与；`hidden_quotas` 只影响显示不影响监测。总开关 `session_quota_notifications` 对四类统一生效，关闭期间不累计用量；Provider 停用 / 移出 sidebar / 步长变化都会重置对应用量基线。
- 阈值设置保存（`SetGlobalQuotaThresholds` / `SetProviderQuotaThresholds`）只产生 Render / PersistSettings / `PublishQuotaSnapshot`（必要时还有 Dynamic 图标更新），不发通知、不触发刷新。`PublishQuotaSnapshot` 经 `ContextCapabilities::publish_quota_snapshot` 路由到 bootstrap 的 Linux D-Bus 即时发射（`bootstrap::workers::linux_dbus` 里以 GPUI Global 注册的共享 handle），非 Linux 为空操作；runtime 不持有 D-Bus handle。

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

- 设置写入由后台 `settings_writer` 串行化并做 debounce；custom-provider 保存所需的 deferred flush 在专用 I/O worker 上等待，不进入 GPUI dispatch 栈，其他前台同步路径（如全局热键）仍可直接调用 `flush()`。脚本 Run Test 使用另一条独立串行队列，长 timeout 不会阻塞 NewAPI / Script Provider 的 save/delete/load。正常退出时，refresh 发送 Shutdown 请求，script-test 发布队外取消状态；custom-provider CRUD 关闭入队端后继续 drain 已接受事务，三者在共同 60ms deadline 内 join，超时线程记录警告并 detach。worker 先把完成 action 写入可靠结果 ledger，再发轻量唤醒；退出阶段会在最终 settings 快照前同步结算 ledger 中已经收到但尚未消费的 action；超时 detach 的 CRUD 不保证完成，也不保证其迟到结果得到结算。Linux D-Bus handle 另最多等待 20ms，超时线程 detach；settings writer 使用独立 `Shutdown` 协议完成 pending snapshot 的 final flush，在正常退出协议之外被独立销毁时使用 80ms 兜底等待。最终 `start_at_login` 状态也在 quit observer 返回前同步确认，保留用户设置的完成保证。应用退出、D-Bus 收尾、设置写入 debounce、settings writer 独立销毁与设置窗口建窗延迟的命名时序参数统一维护在 `src/timing.rs`，修改时应同时复核这里描述的协议。
- `AppSettings` 是运行时领域模型；顶层 JSON 形状由 `settings_store::PersistedAppSettingsV1` 拥有并转换。Provider 的用户布局统一持久化为有序 `provider_layout`（`id` / `in_sidebar` / `enabled`），数组顺序就是排序；旧版 `enabled_providers`、`provider_order`、`sidebar_providers` 只在加载边界迁移，保存时不会继续写回。保存兼容的新版本文件时会递归保留未知字段，同时以当前领域值整体替换 credentials、hidden quotas 和 provider layout，确保删除操作不会被兼容合并恢复。
  - 迁移规则：`in_sidebar = sidebar || enabled`，再交给 `ProviderLayoutItem::new`（该类型仍保证“启用必须出现在 sidebar”）。因此旧配置中某个 Provider 若只在 `enabled_providers` 中启用、但从未加入 `sidebar_providers`，迁移后会保持 `enabled` 并补进 sidebar。旧 Overview / refresh 并不要求 sidebar 成员资格即可监控；若改成只按 sidebar 成员写入 `in_sidebar`，构造器会把 `enabled` 强制关掉，这些用户会静默停止监控。sidebar 中未启用的项迁移后仍是 `in_sidebar = true, enabled = false`。
- 系统深浅色与 Linux GNOME extension 的首份状态在进入 GPUI 前预热（每条平台命令最多 500ms）；进入事件循环后只读取缓存，过期 CLI 刷新在后台运行，不会同步阻塞渲染或托盘更新。
- `settings.json`、BananaTray 代写的外部 OAuth 凭证和自定义 provider YAML 复用私有文件写入原语：同目录临时文件、Unix `0600`、写入同步后 rename，并在可恢复失败路径清理临时文件。脚本 provider 使用不可变版本化脚本，最后原子提交引用它的 YAML；成功后再清理旧脚本，避免崩溃窗口产生跨版本文件对。
- `settings.json` 加载失败（JSON 损坏等）时，启动路径会先把原文件 rename 备份为 `settings.json.corrupt-<epoch>` 再回退默认值，避免后续 persist 覆盖后原始内容不可恢复；备份成功时启动后发送系统通知告知备份位置。
- 外部 provider 的真实认证状态不一定存放在 `settings.json`，也可能来自环境变量、CLI 登录态或 provider 自己的文件。
- 用量历史是另一份本地库 `quota_history.sqlite`，和 `settings.json` 放在同一个配置目录。reducer 只把已采纳的 `Success` / `Unavailable` / `Failed` 交给历史线程；跳过的刷新不写入。失败样本会留下，但不会进入折线。保留天数在 `settings.json` 的 `history.retention_days`（默认 90，范围 1 到 365），provider 可以用 `history_retention_days` 覆盖。历史线程不读设置，cutoff 由 reducer 放进任务。缩短保留会立刻按 provider 删除更早的行；加长不会删数据。设置页读图时，查询起点也不会早于当前保留期限。手动清除和改天数是两件事，清除需要确认。退出时历史线程和其它 worker 共用 60ms，未画完的读取结果直接丢掉，不写回这次会话。
- 折线有两处展示，共用同一套聚合与视图构建（`selectors/history.rs` + `ui/widgets/display/history_chart.rs`）：设置页 provider 详情可切 24 小时 / 7 天 / 30 天，每条序列自带标题；托盘弹窗 provider 页固定近 24 小时，按 `quota_key` 画进对应的配额卡片底部，不再单独占一块，也不再重复标题。纵轴跟着「显示」里的额度显示：剩余或已用，但图上不写这两个字。计量配额的剩余是 `limit - used`；纯余额没有已用，始终画剩余。刻度按可见样本的最小、最大值两侧留白，百分比不再固定成 0–100，刻度文字先取整，取整后上下写成同一个数时再留小数；样本持平时折线画在中间，只标这一个值。横轴是这些样本的起止本地时间，不是整段空白窗口；同一天只标时分，跨天用 `10/4 19:51`。两个采样之间保持上一个真实值，用量相同的采样并成一段平线，数值变化时再跳到新值，台阶带圆角，线下有一层淡填充。平时不画采样点，悬停才标中那一次，不把中间画成渐变斜线。指针停在折线区域（纵轴数字右侧）时，画跟手的虚线指示线，读数吸到最近的真实采样，时间和数值都是那一次的，同一天如 `22:16  64%`，跨天带月日。是否同一天，以及横轴两端文字，都按第一个和最后一个样本，不按留白后的轴端。孤立采样在圆点附近几个像素内仍可读，半径不超过到相邻采样的一半；断档中段和留白远端只标时间。多条线同时命中时，读数块留在指针旁。指示线画在折线下面。绘图区只留底边。离开这块区域十字消失，窗口高度不变。设置页里，折线真有断开时才在图下说明；只有托盘里关掉、这里仍画出的配额时才说明这一点。清除这个服务商的历史放在图下，需要再确认一次。「显示」里的「合并折线」默认打开：单位和含义相同的配额画进同一张图，用不同颜色，图例标名称；单位不同的仍分开。关掉就回到每条配额一张图。托盘仍是每张配额卡一条线，不看这个开关。单位或含义不一致的序列不标纵轴数字。对不上可见配额的序列（例如已隐藏的配额）不出现在弹窗里。打开弹窗、切换 provider 页签、该 provider 的刷新被采纳时都会重新加载；不足两个点的序列不画。窗口高度只为这些内嵌折线额外加高。两侧的加载状态各自计号（`history_ui` 与 `popup_history_ui`），同一条响应按 request_id 各自结算。

## Localization Boundary

Provider 层和 refresh 层尽量只保存稳定语义，不缓存最终展示文案。普通用户向失败提示由 `format_user_failure_message` 使用本地化默认文案生成，不直接展示 `raw_detail`；Debug 诊断路径可单独保留已经脱敏的技术细节。`raw_detail` 禁止包含响应正文、凭据、配置值或其他敏感用户数据。

这带来两个稳定收益：

- 切换语言时无需强制刷新 provider 数据。
- 离线 / 缓存状态仍可在 selector 层重新格式化成当前语言。

## Workaround Register

下面这些 workaround 目前仍是有意保留的实现，不应在“顺手清理”时直接删掉。

**优先级说明**：P1 = 直接影响用户体验；P2 = 防御性，可能已不需要；P3 = 平台/语言限制，短期不会变。

**最近核查日期**：2026-10-08（随 fc-gpui 0.11.1 / fc-ui 0.9.1 升级逐条核对）

**2026-10-08 随上游修复删除的条目**（不再出现在代码里，留档备查）：

- `src/platform/popup_window.rs` 直连 AppKit `setFrame:display:animate:` —— fc-gpui 0.10.0 新增 `Window::resize_anchored(size, ResizeAnchor::TopLeft)`，macOS 同步无动画钉顶边，且 0.10.0 修复了同步 resize 回调被 `try_borrow_mut` 吞掉的问题。已替换；`dispatch2` / `raw-window-handle` 依赖一并移除。实测开关弹窗正常。
- `src/bootstrap/ui_bootstrap.rs` 的 `set_tray_panel_mode(true)` —— fc-gpui 0.10.0 起点击 handler 在建 tray 时接线，macOS 不挂菜单时点击直达 `on_tray_icon_click_event`，panel mode 不再是隐性前置条件。已删除，实测点击可达。（注意：macOS 若将来挂托盘菜单，panel mode 重新变成必需，上游对此组合只打一次性 log 警告。）
- `src/bootstrap/settings_window.rs` 的 `+1px` resize nudge —— 0.10.0 的重入 resize 通知修复 + 建窗强制首绘大概率已根治首屏布局问题；已删除。设置窗口布局经 mock-window UI 回归验证；真实多显示器/主题切换下的视觉复核仍建议做一次人工确认。
- `Cargo.toml` 指向本地 fork 路径的 `fc-gpui` `[patch]` —— 为携带 0.11.1 的两处 macOS objc2 迁移遗留修复（关窗 `let _: () = msg_send![window, autorelease]` 的 '@' vs 'v' debug panic；建窗 `NSAutoreleasePool` 显式 `drain()` + Retained drop 双重释放）。上游已作为 fc-gpui 0.11.2 发布（另见 fc-ui 0.9.2 对齐），patch 行已随本次升级删除，全仓回到纯 crates.io 正式版。

**2026-10-09 随 fc-ui 0.9 官方 API 采纳删除的条目**（fc-ui 0.9.0 补齐了此前应用侧手写的三层 workaround）：

- `src/ui/widgets/controls/input_actions.rs` 与 `providers/shared.rs::register_textarea_actions` 的手写键盘 wiring —— fc-ui 0.9.0 新增公开的 `input_state::wire_actions` / `textarea_state::wire_actions`（自带 key_context / track_focus / track_scroll / disabled 判空，内置组件自身也走同一 helper；input 版补齐了手写版长期缺失的 enter/tab/shift_tab/escape 四个 action）。已整文件 / 整函数删除。
- 各 `InputState` / `TextareaState` 构造点的 `s.placeholder = ...` / `s.content = ...` / `s.trim_on_blur = false` 裸字段赋值 —— fc-ui 0.9.0 新增 `value()` / `placeholder()` / `trim_on_blur()` 等链式 builder，已全量替换。注意 0.9 起 `trim_on_blur` 默认 true 且 blur 时真实生效（0.8 时代该字段是摆设），显式 `trim_on_blur(false)` 的语义从"随手写的保险"变为"实打实关闭 trim"，替换时已逐处保留。
- `src/bootstrap/ui_bootstrap.rs` 装死 `Theme::light()` 后不再切换的 fc-ui 主题安装 —— fc-ui 0.9 起输入框光标 / 选区改用 `tokens.primary` 绘制（0.8 是硬编码蓝），light preset 的 primary 为纯黑，叠在深色输入底色上不可见，属于升级引入的实际回归。修复为 `src/ui/fc_ui_theme.rs::sync_fc_ui_theme()`：按用户主题偏好 + 系统外观选 fc-ui preset 并将 primary 覆盖为 BananaTray accent；同步时机为启动、设置窗口打开、窗口外观变化、Display Tab 切换主题偏好四处（`install_theme` 0.9 起自带 `refresh_windows()`，重装即时生效）。

| 位置 | 目的 | 触发条件 / 根因 | 删除条件 | 创建日期 | 上游追踪 | 优先级 |
|------|------|-----------------|----------|---------|---------|--------|
| `AppView` 在 Overview 停留期间不随展开/折叠改窗口高度 | 展开/折叠只改卡片，不触发原生窗口 resize，避免整窗抖动 | GPUI PopUp 改 `contentSize` / drawable 无法做到无闪的实时长高；0.10.0 的 `resize_anchored` 已提供钉顶边无动画 resize，但"展开跟随高度"的交互改版未做 | 当基于 `resize_anchored` 重做展开/折叠跟随高度并实测稳定无抖 | 2026-08 | fc-gpui macOS PopUp resize（0.10.0 `resize_anchored` 可用） | P1 |
| `src/bootstrap/settings_window.rs` 的 `10ms` 延迟打开 | 避免 tray/popup 关闭与 settings 建窗发生在同一轮前台事件处理里时出现窗口激活/生命周期时序问题 | 历史上（adabraka-gpui 0.5.1 时代）出现过同轮关窗+建窗的 `"window not found"` 问题；当前 fork 代码中 `open_window` 为同步插入 slotmap，同一轮 close+open 拿不同 id，未发现仍然必需的代码证据，但作为零成本保险保留；popup→settings 路径缺少自动化实测手段 | 当 popup→settings 切换路径有可靠的实测覆盖（人或自动化）证明同轮关闭+新建稳定无回归 | 2026-04 | fc-gpui `open_window` 时序（0.11.1 未发现相关问题） | P2 |
| `src/bootstrap/ui_bootstrap.rs` 注册 `on_window_closed` 后延迟调用 `trim_gpu_caches()` | 最后一个 GPUI 窗口关闭后释放 renderer 中闲置的 pooled GPU buffer，降低托盘应用长期后台驻留的 GPU 内存占用 | 上游 trim 是 best-effort 且只回收 idle pool；0.11 起新增 per-window trim（含 Linux WGPU），隐藏窗口在隐藏期即可真正回收，但关窗路径"不要在 teardown 中途 trim"的时序理由不变 | 当 GPUI 自身在窗口关闭后自动回收这些 idle pool，或应用不再长驻后台 | 2026-05 | fc-gpui renderer pool（0.11 已加 per-window trim，关窗语义不变） | P2 |
| `src/bootstrap/event_sources/tray.rs` 在 Linux 安装 tray menu（Open / Settings / Quit）作为 fallback | 为仍不稳定转发 `activate` / `secondary_activate` 的 tray host 保留可达入口，避免用户只能依赖左键点击 | fc-gpui 0.6.1（ksni 0.3.4）已修复 GNOME 等主流 host 的激活转发；但完全不发 Activate 事件的 AppIndicator host 仍存在，菜单是最后兜底 | 当目标 Linux tray host 范围内已验证都会稳定发出 tray click 事件，且移除菜单 fallback 后 Ubuntu / Wayland / X11 实测仍可正常打开 popup / settings | 2026-04 | fc-gpui Linux KSNI（0.6.1 已修主流 host；残留为防御性保留） | P1 |
| `src/tray/linux_popup.rs` 的 `ensure_popup_visible`（显式 `show_window()`/`activate_window()`）+ `src/tray/activation.rs` 激活状态机 | 避免 Ubuntu / Linux 托盘点击后 popup 没被 WM/compositor 浮到前台，或在尚未真正获得焦点时被失焦观察器立即关掉 | fc-gpui 0.10.0 起 X11 建窗已消费 `WindowOptions.show/focus`（创建路径上游已解决）；但这些调用同时服务"隐藏（透明/穿透）→ 复显"的非创建路径，且 Wayland 建窗仍不请求激活（协议层面缺 xdg-activation 集成），两处调用对 Wayland 和复显路径仍然必需 | 当 Wayland 建窗激活由上游 xdg-activation 支持，且隐藏→复显路径有平台级保证 | 2026-05 | fc-gpui X11 show/focus（0.10.0 已修）；Wayland xdg-activation（未实现） | P1 |
| Linux popup 复用窗口；拖动或已有保存位置后隐藏优先使用透明渲染 + 鼠标穿透，头部拖动时短暂抑制 auto-hide 并在抑制期后复查失焦，同时持久化 `settings.display.tray_popup.linux_last_position` | 让 Linux 用户在 Wayland 无法精确初始定位时仍可拖动 popup，并在同一进程内尽量保留窗口管理器放置结果；X11 下可跨重启恢复上次拖动位置 | Wayland `xdg_toplevel` 不允许客户端指定窗口位置，`hide_window()`/`show_window()` 可能重新映射到屏幕中央；fc-gpui 的 layer-shell 支持仅限 wlr 系，GNOME Mutter 不支持，对 GNOME 目标用户不可达；上游文档已明确"透明占位就是 Wayland 正规形态" | 当 GNOME 可用的定位协议（非 wlr-layer-shell）落地且满足托盘弹窗交互 | 2026-05 | Wayland 协议限制（GNOME 无 wlr-layer-shell），上游已确认不追 | P3 |
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
