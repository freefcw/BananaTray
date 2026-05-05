# Refresh Strategy

本文件描述 BananaTray 当前的刷新行为契约。

它关注“什么时候会刷新、哪些请求会被跳过、后台如何把结果送回前台”，而不是协调器内部的逐行实现。

## Trigger Sources

刷新目前来自以下几类触发源：

| 触发源 | 对应原因 | 说明 |
|--------|----------|------|
| 应用启动 | `Startup` | 启动后可请求刷新已启用 provider |
| 周期调度 | `Periodic` | 由后台 scheduler 按间隔触发 |
| 用户手动操作 | `Manual` | 用户点击刷新或 Debug 刷新 |
| Provider 开关变化 | `ProviderToggled` | 启用 / 禁用后同步 refresh 配置或触发刷新 |
| 显式 provider reload | `ReloadProviders` | 重新构建 provider manager 快照 |

## Stable Scheduling Rules

后台调度由 `RefreshScheduler` 决定，稳定规则如下：

- 只会刷新当前启用的 provider。
- 正在刷新中的 provider 会被跳过。
- `Startup` 和 `Periodic` 会应用 cooldown。
- `Manual` 和 `ProviderToggled` 可以跳过 cooldown。
- cooldown = 刷新间隔的一半，最小 30 秒。
- 当自动刷新间隔为 `0` 时，周期刷新被禁用。

这意味着：

- 手动刷新优先保证响应性。
- 周期刷新优先避免短时间重复打同一个上游。

## Execution Model

刷新执行采用前后台分离：

1. 前台 reducer 产生 refresh request
2. request 发送到后台 `RefreshCoordinator`
3. 协调器先做 eligibility 判断
4. 合法请求在后台并发执行
5. 结果通过 `RefreshEvent` 回到前台
6. 前台 reducer 再把结果写回状态并决定是否重绘 / 通知

稳定事实：

- provider 刷新运行在后台，不阻塞 GPUI 前台事件处理。
- 多个 provider 可以并发刷新。
- 后台会为单个 provider 刷新套一层协调器级 timeout guard，避免单个卡死任务把整轮结果拖住。
- 这个 timeout guard 只负责“停止等待并清理 in-flight 状态”，不会强制取消底层已经跑出去的阻塞任务。

## Result Semantics

刷新结果稳定分为几类：

- `Success`
- `Unavailable`
- `Failed`
- `SkippedCooldown`
- `SkippedInFlight`
- `SkippedDisabled`

前台依赖这些稳定语义来更新状态、决定错误展示和是否发送告警，而不是依赖 provider 的原始错误字符串。

## Config Sync

前台设置变化不会直接改后台 scheduler 内存，而是通过显式请求同步：

- 刷新间隔变化
- 已启用 provider 列表变化
- BananaTray 托管的 provider credentials 变化

后台在收到新的配置后更新：

- `interval_mins`
- enabled provider 列表
- 周期 deadline
- `ProviderSettings` credentials 快照，并调用 `ProviderManager::sync_provider_credentials()`

这保证了前后台对“谁应该刷新、多久刷新一次”保持一致。

需要 app-managed token override 的 provider（例如 Copilot）必须实现 `AiProvider::sync_provider_credentials()`，在后台 provider 实例中保存线程安全快照。设置页展示 token 状态和后台 refresh 读取 token 不是同一条调用栈；只改 UI 状态不会自动改变后台刷新凭证。

## Custom Provider Reload

`ReloadProviders` 的稳定语义是：

- 重建 `ProviderManager` 快照
- 用新的 provider 状态列表替换旧快照
- 清理 scheduler 中已不存在 provider 的残留状态
- 把 `ProvidersReloaded` 事件发回前台

当前要特别注意：

- BananaTray **没有**监视 providers 目录的自动文件 watcher。
- 应用内通过 NewAPI 保存 / 删除会显式触发 reload。
- 手工编辑 YAML 文件后，通常需要重启应用才能看到变化。

## Debug Refresh

Debug Tab 的单 provider 调试刷新属于 `Manual` 刷新：

- 会临时把日志级别提升到 `Debug`
- 会清空并启用内存日志捕获
- 刷新结束后恢复之前的日志级别

因此 Debug 刷新更适合排查单个 provider 的认证、网络和解析问题，而不是观察周期调度行为。

## What This Doc Avoids

为了降低漂移成本，本文件不再维护以下内容：

- 大型流程图
- 协调器内部逐函数调用链
- 精确的线程池实现细节
- “某个 request 一定由哪个具体文件发起”的文件级说明

如果你需要改 refresh 实现，先看这里的行为契约，再去看 `src/refresh/README.md` 和当前代码。
