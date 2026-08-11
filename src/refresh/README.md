# src/refresh/

后台周期性刷新系统，负责 Provider 数据的定时拉取和并发管理。

## 模块结构

### `types.rs` — 消息类型

通信协议，连接 UI 层和后台刷新线程：

- **`RefreshRequest`** — UI → 协调器的请求：`RefreshAll` / `RefreshOne` / `UpdateConfig` / `ReloadProviders` / `Shutdown`。`RefreshAll` 显式携带本次目标 ID，发送失败时前台可逐个回收 `Refreshing`；`UpdateConfig` 同步刷新间隔、启用 provider 列表和 `ProviderSettings` credentials 快照。
- **`RefreshEvent`** — 协调器 → UI 的事件：`Started` / `Finished(RefreshOutcome)` / `ProvidersReloaded`
- **`RefreshResult`** — 单个 Provider 刷新结果：`Success` / `Unavailable` / `Failed` / `SkippedCooldown` / `SkippedInFlight` / `SkippedDisabled` / `SkippedStale`
- **`RefreshReason`** — 触发原因：`Startup` / `Periodic` / `Manual` / `ProviderToggled`

### `scheduler.rs` — 调度决策引擎

纯逻辑调度器，不执行 I/O：

- 维护每个 Provider 的 cooldown 和 in-flight 状态
- 基于绝对 deadline 的周期定时器（不受异步请求干扰）
- 决定哪些 Provider 可以刷新、何时触发下次周期刷新

### `coordinator.rs` — 协调器（事件循环）

后台线程上运行的事件循环：

- 接收 `RefreshRequest`，通过 `ProviderManagerHandle` 读取当前 `ProviderManager` 快照并执行刷新
- 处理 `UpdateConfig` 时保存最新的 app-managed credentials 快照并更新调度配置；实际执行 refresh 时再通过 `ProviderExecutionContext` 显式传给 provider，确保后台读取到设置页保存的 token override
- 通过 `smol::unblock` 并发执行不同 Provider；同一 Provider 始终保持 single-flight
- 主循环只负责接收请求和任务消息，不等待 Provider 完成，因此活跃刷新期间仍可处理配置、reload 和 shutdown
- 对每个 Provider 刷新施加协调器级 timeout guard；timeout 会及时向前台报告失败，但底层任务真实结束前不会释放 single-flight
- 配置凭证、启用列表或 Provider registry 变化时递增 generation，立即让前台退出 `Refreshing`，并丢弃旧 generation 的迟到结果
- 接收 `ProviderManager` 已分类好的 `ProviderResult<RefreshData>`，不再在 refresh 边界处理裸 `anyhow::Result`
- 将结果封装为 `RefreshEvent` 发回 UI 线程
- 管理 `ProviderManager` 的热重载（自定义 Provider 文件变更）并原子替换共享快照，保证前后台看到的是同一个 registry

测试文件：`coordinator_tests.rs`

## 数据流

```
UI Thread                          Background Thread
─────────                          ─────────────────
RefreshRequest ──(channel)──→ RefreshCoordinator
                                    ├─ UpdateConfig: store credentials + scheduler config
                                    ├─ scheduler 决策
                                    ├─ ProviderManager.refresh_by_id()
                                    └─ RefreshEvent ──(channel)──→ UI
                                         → AppAction::RefreshEventReceived
                                           → reducer → effects
```

## 约束

- 协调器运行在独立线程，通过 `smol::channel` 与 UI 通信
- 请求通道为 unbounded：请求体小、产生速率受 UI 交互自然约束，有界队列的"满"状态只会制造瞬态发送失败（甚至静默丢弃 `UpdateConfig`）；发送失败仅意味着协调器线程已终止
- Provider 刷新通过 `smol::block_on` + `smol::unblock` 执行异步代码
- Cooldown 机制防止短时间内对同一 Provider 重复刷新
- Provider panic 会被转换为 `RefreshResult::Failed` 并在任务结束时释放 single-flight；timeout 只报告 UI 终态，底层任务结束后才释放 single-flight
- panic 兜底依赖 panic 策略保持 `unwind`（Cargo 默认）：`panic = "abort"` 会让 provider 的 panic 直接终止整个托盘进程，且该行为只在 release 构建出现、dev 测试无法发现。该契约由 `scripts/check-release-panic-profile.sh` 在 CI 守护，勿在 Cargo.toml 引入 `panic = "abort"`
