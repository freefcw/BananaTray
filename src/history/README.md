# src/history/

用量历史。一次已经被 reducer 采纳的刷新，变成可查询的本地事实。

## 边界

- `sample` 把 `Success` / `Unavailable` / `Failed` 收成一条样本。`Skipped*` 返回 `None`，不落库。
- `retention` 只计算 cutoff 和查询窗口。SQLite 不读 `AppSettings`。
- `series` 把样本收成折线。失败样本不进入折线。
- `sqlite` 是唯一的存储适配器。前台 reducer 和设置页都不碰数据库。
- 脱敏后的 `detail` 最长 160 个字符。不保存账号邮箱、token 或 cookie。

## 保留

全局默认 90 天，合法范围是 1 到 365。0 不是永久保留。Provider 没有覆盖时跟随全局。

读图时，查询起点是 `max(选中范围起点, 保留截止)`。横轴仍是用户选中的范围。

自动清理按 provider 删除 `captured_at_ms < cutoff` 的行。不扫全表，也不 `VACUUM`。

## 文件

数据库在配置目录的 `quota_history.sqlite`。打开失败时应用继续启动，这次进程里的历史读取返回不可用。
