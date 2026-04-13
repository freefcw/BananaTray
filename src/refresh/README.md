# src/refresh/

后台周期性刷新系统，负责 Provider 数据的定时拉取和并发管理。

## 模块结构

### `types.rs` — 消息类型

通信协议，连接 UI 层和后台刷新线程：

- **`RefreshRequest`** — UI → 协调器的请求：`RefreshAll` / `RefreshOne` / `UpdateConfig` / `ReloadProviders` / `Shutdown`
- **`RefreshEvent`** — 协调器 → UI 的事件：`Started` / `Finished(RefreshOutcome)` / `ProvidersReloaded`
- **`RefreshResult`** — 单个 Provider 刷新结果：`Success` / `Unavailable` / `Failed` / `SkippedCooldown` / `SkippedInFlight` / `SkippedDisabled`
- **`RefreshReason`** — 触发原因：`Startup` / `Periodic` / `Manual` / `ProviderToggled`

### `scheduler.rs` — 调度决策引擎

纯逻辑调度器，不执行 I/O：

- 维护每个 Provider 的 cooldown 和 in-flight 状态
- 基于绝对 deadline 的周期定时器（不受异步请求干扰）
- 决定哪些 Provider 可以刷新、何时触发下次周期刷新

### `coordinator.rs` — 协调器（事件循环）

后台线程上运行的事件循环：

- 接收 `RefreshRequest`，委托 `ProviderManager` 执行刷新
- 通过 `smol::unblock` 并发执行多个 Provider 刷新
- 将结果封装为 `RefreshEvent` 发回 UI 线程
- 管理 `ProviderManager` 的热重载（自定义 Provider 文件变更）

测试文件：`coordinator_tests.rs`

## 数据流

```
UI Thread                          Background Thread
─────────                          ─────────────────
RefreshRequest ──(channel)──→ RefreshCoordinator
                                    ├─ scheduler 决策
                                    ├─ ProviderManager.refresh_by_id()
                                    └─ RefreshEvent ──(channel)──→ UI
                                         → AppAction::RefreshEventReceived
                                           → reducer → effects
```

## 约束

- 协调器运行在独立线程，通过 `crossbeam_channel` 与 UI 通信
- Provider 刷新通过 `smol::block_on` + `smol::unblock` 执行异步代码
- Cooldown 机制防止短时间内对同一 Provider 重复刷新
