# src/history/

用量历史。一次已经被 reducer 采纳的刷新，变成可查询的本地事实。

## 边界

- `sample` 把 `Success` / `Unavailable` / `Failed` 收成一条样本。`Skipped*` 返回 `None`，不落库。
- `retention` 只计算 cutoff 和查询窗口。SQLite 不读 `AppSettings`。历史 reducer 使用 dispatch 显式传入的 epoch 毫秒，采样和同轮查询共用时间快照。
- `HistoryLoadRequest` 与完成 action 透传 `HistoryScope`（Settings / Popup）；独立计号的两个查看范围只结算自己的响应，即使 Provider 和请求编号相同也不会串线。
- `series` 把样本收成折线。失败样本不进入折线。
- `period` 给托盘配额卡的「本周期」折线推断配额当前周期：查询窗口固定 31 天（盖住一个自然月），周期终点取最新采样的重置时间，起点按标签长度（session 5h / daily 24h / weekly 7d / monthly 一个自然月）→ `quota_type` → 同一重置时间（±10 分钟）连续采样能盖住的最小周期顺序推；桶宽与设置页同长度范围一致。认不出周期的配额回退近 24 小时，设置页不提供这个范围。
- `sqlite` 是唯一的存储适配器。前台 reducer 和设置页都不碰数据库。
- 脱敏后的 `detail` 最长 160 个字符。不保存账号邮箱、token 或 cookie。

## 失败样本字段

失败样本（`HistoryStatus::Failed`）会一并落库 `error_kind`、`failure_reason`、
`failure_reason_payload`、`failure_advice_json`、`detail`、`refresh_reason`。
这些字段目前仅供调试与 issue 取证使用，UI 的折线/摘要只消费 `captured_at_ms`、`status` 与 `points`，
不读任何失败明细字段。改动 UI 消费路径前先确认这两个事实，避免对着无人读取的字段调文案。

## 保留

全局默认 90 天，合法范围是 1 到 365。0 不是永久保留。Provider 没有覆盖时跟随全局。

读图时，查询起点是 `max(选中范围起点, 保留截止)`。查询窗口仍是用户选中的范围；画出来的横轴是这段窗口里实际样本的起止时间。

自动清理按 provider 删除 `captured_at_ms < cutoff` 的行。不扫全表，也不 `VACUUM`。

## 文件

数据库在配置目录的 `quota_history.sqlite`。打开失败时应用继续启动，这次进程里的历史读取返回不可用。
