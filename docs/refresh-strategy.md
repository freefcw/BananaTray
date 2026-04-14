# 刷新策略文档

## 概述

BananaTray 的刷新系统是一个基于后台线程的周期性数据拉取机制，负责定期从各个 AI 编程助手提供商获取配额信息。该系统采用 **Action-Reducer-Effect** 架构模式，将 UI 交互、状态管理和副作用执行完全解耦，确保系统的可测试性和可维护性。

## 系统架构

### 架构图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              UI Thread (GPUI)                               │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                  │
│  │   AppView    │───▶│   Reducer    │───▶│   Effects    │                  │
│  │   (用户交互)  │    │  (状态转换)   │    │  (副作用执行)  │                  │
│  └──────────────┘    └──────────────┘    └──────────────┘                  │
│         │                   │                   │                            │
│         │ AppAction         │ AppEffect         │ RefreshRequest             │
│         ▼                   ▼                   ▼                            │
│  ┌────────────────────────────────────────────────────────────┐            │
│  │                    AppSession (纯逻辑状态)                   │            │
│  │  - ProviderStore (Provider 状态列表)                         │            │
│  │  - NavigationState (导航状态)                               │            │
│  │  - AlertTracker (配额告警状态机)                             │            │
│  └────────────────────────────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ smol::channel
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Background Thread (刷新协调器)                         │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                  │
│  │ Coordinator  │───▶│  Scheduler   │───▶│ProviderManager│                  │
│  │ (事件循环)    │    │ (调度决策)    │    │ (数据拉取)    │                  │
│  └──────────────┘    └──────────────┘    └──────────────┘                  │
│         │                   │                   │                            │
│         │ RefreshEvent      │ eligibility       │ RefreshData                │
│         ▼                   ▼                   ▼                            │
│  ┌────────────────────────────────────────────────────────────┐            │
│  │                    RefreshScheduler                         │            │
│  │  - last_refreshed (上次成功刷新时间)                        │            │
│  │  - in_flight (正在刷新标志)                                │            │
│  │  - next_periodic (下次周期刷新绝对时间)                     │            │
│  └────────────────────────────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 核心组件

| 组件 | 位置 | 职责 | 线程 |
|------|------|------|------|
| **AppAction** | `application/action.rs` | 定义所有用户操作和系统事件 | UI |
| **Reducer** | `application/reducer.rs` | 纯函数状态转换逻辑 | UI |
| **AppEffect** | `application/effect.rs` | 副作用声明 | UI |
| **Runtime** | `runtime/mod.rs` | Effect 执行器 | UI |
| **RefreshCoordinator** | `refresh/coordinator.rs` | 后台事件循环 + 并发刷新 | Background |
| **RefreshScheduler** | `refresh/scheduler.rs` | 调度决策引擎（纯逻辑） | Background |
| **ProviderManager** | `providers/mod.rs` | Provider 实例管理和数据拉取 | Background |

## 刷新流程

### 完整流程图

```
用户操作/系统事件
        │
        ▼
┌───────────────┐
│  AppAction    │
│  - RefreshAll │
│  - RefreshOne │
│  - ToggleProvider
└───────┬───────┘
        │
        ▼
┌───────────────┐
│   Reducer     │  纯函数转换
│ (reducer.rs)  │
└───────┬───────┘
        │
        ▼
┌───────────────┐
│  AppEffect    │
│SendRefreshReq │
└───────┬───────┘
        │
        ▼
┌───────────────┐
│   Runtime     │  Effect 执行器
│ (runtime/mod) │
└───────┬───────┘
        │
        │ smol::channel
        ▼
┌───────────────────────────────────────┐
│      RefreshCoordinator              │
│     (coordinator.rs)                  │
│  ┌─────────────────────────────────┐  │
│  │  事件循环 (run())               │  │
│  │  - 接收 RefreshRequest          │  │
│  │  - 等待周期定时器               │  │
│  └─────────────┬───────────────────┘  │
└────────────────┼──────────────────────┘
                 │
                 ▼
┌───────────────────────────────────────┐
│      RefreshScheduler                │
│     (scheduler.rs)                    │
│  ┌─────────────────────────────────┐  │
│  │  check_eligibility()            │  │
│  │  - 检查是否启用                 │  │
│  │  - 检查是否正在刷新             │  │
│  │  - 检查是否在 cooldown          │  │
│  └─────────────┬───────────────────┘  │
└────────────────┼──────────────────────┘
                 │
                 │ 符合条件
                 ▼
┌───────────────────────────────────────┐
│      并发刷新执行                     │
│  ┌─────────────────────────────────┐  │
│  │  execute_refresh_concurrent()   │  │
│  │  - Phase 1: 过滤 + 发送 Started │  │
│  │  - Phase 2: smol 并发           │  │
│  │  - Phase 3: 收集结果            │  │
│  └─────────────┬───────────────────┘  │
└────────────────┼──────────────────────┘
                 │
                 ▼
┌───────────────────────────────────────┐
│      ProviderManager                  │
│  - refresh_by_id()                    │
│  - 调用具体 Provider 实现             │
│  - 返回 RefreshData                   │
└────────────────┼──────────────────────┘
                 │
                 │ RefreshEvent
                 ▼
┌───────────────────────────────────────┐
│      UI Thread (事件泵)               │
│  start_event_pump()                   │
│  - 接收 RefreshEvent                  │
│  - 分发到 RefreshEventReceived Action │
└────────────────┼──────────────────────┘
                 │
                 ▼
┌───────────────┐
│   Reducer     │  处理 RefreshEvent
│ - 更新状态    │
│ - 触发通知    │
└───────────────┘
```

### 触发源分类

刷新操作可以由以下触发源发起：

| 触发源 | Action | Reason | 说明 |
|--------|--------|--------|------|
| **启动** | RefreshAll | Startup | 应用启动时立即刷新所有已启用 Provider |
| **周期** | (定时器) | Periodic | 根据配置的间隔自动触发 |
| **手动** | RefreshAll / RefreshOne | Manual | 用户点击刷新按钮 |
| **切换** | ToggleProvider | ProviderToggled | 用户启用/禁用 Provider 时触发 |
| **热重载** | ReloadProviders | - | 自定义 Provider 文件变更后重载 |

## 调度策略

### RefreshScheduler 核心逻辑

`RefreshScheduler` 是一个纯逻辑调度器，负责决定每个 Provider 是否可以被刷新。

#### 状态管理

```rust
pub struct RefreshScheduler {
    last_refreshed: HashMap<ProviderId, Instant>,  // 上次成功刷新时间
    in_flight: HashMap<ProviderId, bool>,          // 是否正在刷新
    interval_mins: u64,                            // 刷新间隔（分钟）
    enabled_providers: Vec<ProviderId>,            // 已启用列表
    next_periodic: Instant,                        // 下次周期刷新绝对时间
}
```

#### Cooldown 策略

- **Cooldown 时长** = 刷新间隔的一半，最小 30 秒
  - 例如：间隔 5 分钟 → Cooldown 2.5 分钟
  - 例如：间隔 1 分钟 → Cooldown 30 秒（最小值）

#### Eligibility 检查

`check_eligibility()` 按以下顺序检查：

1. **是否启用** → 未启用返回 `SkippedDisabled`
2. **是否正在刷新** → 正在刷新返回 `SkippedInFlight`
3. **是否在 Cooldown** → 仅对 `Periodic` 和 `Startup` 触发源检查，返回 `SkippedCooldown`
4. **手动刷新** → 忽略 Cooldown 限制

```rust
pub fn check_eligibility(
    &self,
    id: &ProviderId,
    reason: RefreshReason,
) -> Option<RefreshResult> {
    if !self.enabled_providers.contains(id) {
        return Some(RefreshResult::SkippedDisabled);
    }
    if self.is_in_flight(id) {
        return Some(RefreshResult::SkippedInFlight);
    }
    if matches!(reason, RefreshReason::Periodic | RefreshReason::Startup)
        && self.is_on_cooldown(id)
    {
        return Some(RefreshResult::SkippedCooldown);
    }
    None  // 可以刷新
}
```

#### 周期定时器

使用**绝对 deadline** 而非相对定时器，避免收到请求时重置定时器：

```rust
pub async fn run(mut self) {
    loop {
        // 计算距离下次周期的剩余时间
        let wait = self.scheduler.time_until_next_periodic();

        // 等待请求或定时器
        let request = smol::future::or(
            async { Some(self.request_rx.recv().await) },
            async { smol::Timer::after(wait).await; None }
        ).await;

        match request {
            None => {
                // 定时器触发 → 周期刷新
                self.execute_refresh_concurrent(...);
                self.scheduler.advance_periodic_deadline();
            }
            Some(Ok(req)) => {
                // 处理请求
                match req {
                    RefreshRequest::RefreshAll { reason } => {
                        self.execute_refresh_concurrent(...);
                        // 手动刷新后重置周期定时器
                        if matches!(reason, RefreshReason::Manual) {
                            self.scheduler.advance_periodic_deadline();
                        }
                    }
                    // ...
                }
            }
        }
    }
}
```

### 并发执行策略

`execute_refresh_concurrent()` 采用三阶段并发模式：

```
Phase 1: 过滤 + 标记
  ┌─────────────────────────────────┐
  │  for each provider:             │
  │    - check_eligibility()        │
  │    - send Skip event if needed  │
  │    - mark_in_flight()           │
  │    - send Started event         │
  └─────────────────────────────────┘
                │
                ▼
Phase 2: 并发执行
  ┌─────────────────────────────────┐
  │  for each eligible provider:    │
  │    smol::spawn(async {           │
  │      smol::unblock(|| {          │
  │        mgr.refresh_by_id()       │
  │      })                          │
  │      → send outcome to channel  │
  │    }).detach()                   │
  └─────────────────────────────────┘
                │
                ▼
Phase 3: 结果收集
  ┌─────────────────────────────────┐
  │  while let Ok(outcome) =        │
  │      result_rx.recv().await     │
  │    record_outcome(outcome)      │
  │      - clear_in_flight()        │
  │      - record_success()         │
  │      - send Finished event      │
  └─────────────────────────────────┘
```

## 消息类型

### RefreshRequest (UI → Coordinator)

```rust
pub enum RefreshRequest {
    RefreshAll { reason: RefreshReason },
    RefreshOne { id: ProviderId, reason: RefreshReason },
    UpdateConfig { interval_mins: u64, enabled: Vec<ProviderId> },
    ReloadProviders,  // 热重载自定义 Provider
    Shutdown,
}
```

### RefreshEvent (Coordinator → UI)

```rust
pub enum RefreshEvent {
    Started { id: ProviderId },
    Finished(RefreshOutcome),
    ProvidersReloaded { statuses: Vec<ProviderStatus> },
}
```

### RefreshResult

```rust
pub enum RefreshResult {
    Success { data: RefreshData },
    Unavailable { message: String },
    Failed { error: String, error_kind: ErrorKind },
    SkippedCooldown,
    SkippedInFlight,
    SkippedDisabled,
}
```

## 配置管理

### 刷新间隔配置

| 配置项 | 位置 | 默认值 | 说明 |
|--------|------|--------|------|
| `refresh_interval_mins` | `AppSettings.system` | 5 分钟 | 0 表示禁用自动刷新 |

### 配置同步流程

当用户修改刷新间隔或启用/禁用 Provider 时：

```
SettingChange::RefreshCadence(mins)
    │
    ▼
Reducer
    │
    ├─ session.settings.system.refresh_interval_mins = mins
    │
    └─ AppEffect::SendRefreshRequest(UpdateConfig { interval_mins, enabled })
          │
          ▼
    Runtime: send_refresh_request()
          │
          ▼
    RefreshCoordinator
          │
          ▼
    RefreshScheduler::update_config(interval_mins, enabled)
          │
          ├─ interval_mins = interval_mins
          ├─ enabled_providers = enabled
          └─ if interval_changed:
                 next_periodic = now + periodic_duration()
```

## 热重载机制

### 自定义 Provider 热重载流程

当用户添加/编辑/删除自定义 Provider YAML 文件时：

```
SubmitNewApi / DeleteNewApi
    │
    ▼
Reducer
    │
    ├─ 预注册 Provider ID 到 settings
    ├─ AppEffect::SaveCustomProviderYaml / DeleteCustomProviderYaml
    └─ AppEffect::PersistSettings
          │
          ▼
Runtime
    │
    ├─ 写入/删除 YAML 文件
    └─ AppEffect::SendRefreshRequest(ReloadProviders)
          │
          ▼
RefreshCoordinator
    │
    ├─ 重建 ProviderManager
    ├─ scheduler.cleanup_stale()  // 清理残留状态
    ├─ RefreshEvent::ProvidersReloaded { statuses }
    └─ manager = new_manager
          │
          ▼
UI Thread (事件泵)
    │
    ▼
Reducer::RefreshEventReceived(ProvidersReloaded)
    │
    ├─ provider_store.sync_custom_providers(&statuses)
    ├─ settings.prune_stale_custom_ids()
    ├─ 自动启用新增的 Provider
    ├─ cleanup_dangling_refs()  // 清理导航引用
    ├─ SendRefreshRequest(UpdateConfig)  // 同步配置
    └─ 对新增/更新的 Provider 立即刷新
```

### 热重载的三层自动注册

1. **启动时** (`AppSession::new`): 磁盘上存在但未在 `enabled_providers` 中的 YAML 文件自动启用
2. **保存时** (`SubmitNewApi`): Provider ID 预注册到 `enabled_providers` + `sidebar_providers`，热重载后立即可见
3. **热重载时** (`ProvidersReloaded`): 手动添加的 YAML 文件自动启用并加入 sidebar

## 状态管理

### Provider 状态生命周期

```
┌─────────┐
│ Initial │
└────┬────┘
     │
     ▼
┌─────────┐    RefreshEvent::Started
│Loading  │◀───────────────────────────┐
└────┬────┘                            │
     │                                 │
     ▼                                 │
┌─────────┐    RefreshResult           │
│ Success │◀───────────────────────────┘
└─────────┘
     │
     ├─ mark_refresh_succeeded(data)
     ├─ alert_tracker.update()
     └─ 可能触发 QuotaNotification

┌─────────┐    RefreshResult::Unavailable
│Unavailable│
└─────────┘
     │
     └─ mark_unavailable(message)

┌─────────┐    RefreshResult::Failed
│  Failed │
└─────────┘
     │
     └─ mark_refresh_failed(error, error_kind)
```

### 动态图标更新策略

Dynamic 模式下的托盘图标根据当前 Provider 的配额状态自动更新：

```rust
fn sync_dynamic_icon_if_needed(
    session: &AppSession,
    refreshed_id: &ProviderId,
    prev_status: StatusLevel,
    effects: &mut Vec<AppEffect>,
) {
    // 仅在 Dynamic 模式下
    if session.settings.display.tray_icon_style != TrayIconStyle::Dynamic {
        return;
    }
    // 弹窗可见时延迟更新（关闭时同步）
    if session.popup_visible {
        return;
    }
    // 只响应当前 Provider 的刷新
    if *refreshed_id != session.nav.last_provider_id {
        return;
    }
    // 状态变化时才更新
    let new_status = session.current_provider_status();
    if new_status != prev_status {
        effects.push(
            ContextEffect::ApplyTrayIcon(
                TrayIconRequest::DynamicStatus(new_status)
            ).into()
        );
    }
}
```

## 调试刷新

### Debug Tab 刷新机制

Debug Tab 提供强制刷新功能，跳过 cooldown 并临时提升日志级别：

```
DebugRefreshProvider Action
    │
    ▼
Reducer
    │
    ├─ debug_ui.refresh_active = true
    ├─ provider_store.mark_refreshing_by_id(id)
    └─ AppEffect::StartDebugRefresh(id)
          │
          ▼
Runtime
    │
    ├─ 保存当前日志级别到 debug_ui.prev_log_level
    ├─ LogCapture::global().clear()
    ├─ LogCapture::global().enable()
    ├─ log::set_max_level(Debug)
    └─ SendRefreshRequest(RefreshOne { id, Manual })
          │
          ▼
RefreshCoordinator (跳过 cooldown，因为 reason = Manual)
    │
    ▼
RefreshEvent::Finished
    │
    ▼
Reducer (检测到 is_debug_target)
    │
    ├─ debug_ui.refresh_active = false
    ├─ AppEffect::RestoreLogLevel(prev_level)
    └─ LogCapture::global().disable()
```

## 错误处理

### ProviderError 分类

`ProviderError` 定义在 `providers/mod.rs`，包含面向用户的提示（国际化）和技术性错误（保留英文）：

**面向用户的提示（国际化）：**
- `CliNotFound` - CLI 未安装或找不到
- `AuthRequired` - 需要登录认证
- `SessionExpired` - OAuth 会话已过期
- `FolderTrustRequired` - 需要信任文件夹（Claude CLI 特有）
- `UpdateRequired` - CLI 需要更新
- `ConfigMissing` - 配置缺失

**技术性错误（不国际化，保留英文）：**
- `Unavailable` - Provider 在当前环境不可用
- `ParseFailed` - 解析响应失败
- `Timeout` - 网络请求超时
- `NoData` - 无配额数据
- `NetworkFailed` - 网络请求失败
- `FetchFailed` - 其他获取失败

`ProviderErrorPresenter`（`providers/error_presenter.rs`）负责将错误转换为用户友好的消息和 `ErrorKind` 分类：

| ProviderError | ErrorKind | 说明 |
|---------------|-----------|------|
| `ConfigMissing` | `ConfigMissing` | 配置错误 |
| `AuthRequired` / `SessionExpired` | `AuthRequired` | 认证错误 |
| `Timeout` / `NetworkFailed` | `NetworkError` | 网络错误 |
| 其他 | `Unknown` | 未知错误 |

### 错误转换流程

```
ProviderManager::refresh_by_id()
    │
    ├─ 返回 anyhow::Result<RefreshData>
    │
    ▼
RefreshCoordinator::build_outcome()
    │
    ├─ Ok(data) → RefreshResult::Success { data }
    │
    └─ Err(err)
          │
          ▼
    ProviderError::classify(&err)
          │
          ├─ Unavailable → RefreshResult::Unavailable { message }
          └─ 其他 → RefreshResult::Failed { error, error_kind }
```

## 性能优化

### 并发刷新

- 使用 `smol::spawn` + `smol::unblock` 并发执行多个 Provider 刷新，结果按完成顺序回传
- 通过 `smol::channel` 收集结果，避免慢 Provider 阻塞已完成 Provider 的状态上报
- Phase 1 提前发送 `Started` 事件，UI 立即响应

### 绝对定时器

- 使用 `Instant` 绝对时间而非相对定时器
- 避免收到请求时重置定时器导致周期漂移

### 最小 Cooldown

- Cooldown 最小 30 秒，防止过于频繁的刷新
- 刷新间隔的一半，但受 MIN_COOLDOWN_SECS 限制

## 测试覆盖

| 模块 | 测试文件 | 覆盖内容 |
|------|----------|----------|
| `scheduler.rs` | 内置 `#[cfg(test)]` | Cooldown 计算、Eligibility 检查、Config 更新 |
| `coordinator.rs` | `coordinator_tests.rs` | 结果转换、事件发送 |
| `reducer.rs` | `reducer_tests.rs` | Action-Reducer-Effect 流程、刷新事件处理 |

## 启动流程

```
main()
    │
    ▼
bootstrap()
    │
    ├─ load_settings()
    ├─ sync_initial_auto_launch()
    ├─ bootstrap_ui()
    │
    ├─ bootstrap_refresh()
    │       │
    │       ├─ ProviderManager::new()
    │       ├─ RefreshCoordinator::new()
    │       ├─ std::thread::spawn(coordinator.run())
    │       └─ 返回 (refresh_tx, event_rx, manager)
    │
    ├─ start_event_pump(event_rx)
    │       │
    │       └─ 前台 executor 接收 RefreshEvent
    │           └─ 分发到 RefreshEventReceived Action
    │
    └─ trigger_initial_refresh()
            │
            ├─ SendRefreshRequest(UpdateConfig)
            └─ SendRefreshRequest(RefreshAll { Startup })
```

## 约束与限制

1. **GPUI 隔离**：`RefreshCoordinator` 运行在独立线程，不依赖 GPUI
2. **纯逻辑模块**：`RefreshScheduler` 无 async、无 IO，完全可同步测试
3. **RefCell 安全**：Effect handler 不得调用 `dispatch_*` 函数，使用 `schedule_*` 延迟
4. **最小 Cooldown**：30 秒，防止过于频繁的刷新
5. **自动刷新禁用**：间隔为 0 时，检查间隔为 3600 秒（1 小时）

## 相关文档

- [架构文档](architecture.md) - 整体架构说明
- [Provider 文档](providers.md) - Provider 实现指南
- [应用层文档](../src/application/README.md) - Action-Reducer-Effect 架构
- [刷新模块文档](../src/refresh/README.md) - 刷新模块详解
