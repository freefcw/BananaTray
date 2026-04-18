# Codeium-family Providers

本文件说明 BananaTray 当前对 Antigravity / Windsurf 的共享实现方式。

它是专题参考文档，不是 provider 注册表的完整契约。对外稳定边界请以 `docs/providers.md` 为准。

## 当前定位

BananaTray 把 **Antigravity** 和 **Windsurf** 视为两个独立的 built-in provider：

- UI 中独立展示
- 各自拥有独立的 metadata、图标和可用性判断
- 共享一套底层 Codeium-family 实现

共享实现位于 `src/providers/codeium_family/`，具体 provider facade 分别位于 `src/providers/antigravity/` 和 `src/providers/windsurf.rs`。

## Stable Design

共享层只处理长期稳定的共性：

- 本地 language server 进程发现
- 本地接口调用
- 本地 cache fallback
- JSON / protobuf 解析

provider facade 只负责提供产品差异：

- provider kind 与展示元数据
- `ide_name`
- cache DB 相对路径
- auth status key 候选
- 进程识别 marker
- provider-specific unavailable message

这意味着：

- 不要把 Windsurf 折叠成 Antigravity 的别名。
- 也不要把产品差异反向塞进共享流程逻辑里。

## Refresh Path

当前 refresh 策略保持为：

1. 优先尝试 live source
2. live source 失败时回退本地 cache
3. 两条路径都失败时返回结构化错误

这里的关键不是“两个 provider 完全相同”，而是“它们共享同一个 fallback 形状”。

## Stable Difference Dimensions

当前真正稳定、值得文档化的差异维度只有这些：

- provider 身份与展示名
- 本地 cache DB 路径
- auth status key 候选
- 进程参数 / 路径 marker
- dashboard URL

如果未来还有差异，应优先继续加到 spec，而不是复制整套 provider 实现。

## Runtime Validation

当你修改 Codeium-family 实现后，建议在本机做一次运行时校验：

```bash
cargo run -- debug-codeium-family all
cargo run -- debug-codeium-family antigravity
cargo run -- debug-codeium-family windsurf
```

这个命令适合检查：

- cache DB 是否存在
- 关键 key 是否存在
- 进程 marker 是否仍能识别
- 端口 / csrf token 是否还能提取
- endpoint 提示是否合理

## Known Limits

- 本地服务的参数格式和 marker 可能随上游版本变化。
- 本地 cache key 名称可能因产品版本变化而漂移。
- 本地 HTTPS endpoint 可能使用自签证书。
- cache fallback 只能反映本地已缓存的数据，不保证和实时服务完全一致。

## Maintenance Rule

如果你只是新增一个普通 provider，不需要读这份文档。

只有在以下场景才需要同步更新这里：

- 修改 `codeium_family` 共享层边界
- 修改 Antigravity / Windsurf 的差异建模方式
- 修改运行时校验命令或关键诊断入口
