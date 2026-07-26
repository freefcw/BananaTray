# Logging

本文件只记录当前可依赖的日志行为和调试入口。

## Current Behavior

- 应用日志同时输出到：
  - stdout
  - 日志文件
- 默认日志级别是 `info`
- 启动时可通过 `RUST_LOG` 设定初始级别
- 运行中可在 Debug Tab 临时或手动调整当前进程的日志级别

## Log File Location

默认日志文件名为 `bananatray.log`。

典型位置：

- macOS: `~/Library/Logs/bananatray/bananatray.log`
- Linux: `$XDG_STATE_HOME/bananatray/bananatray.log`

如果设置了 `BANANATRAY_LOG_DIR`，日志会写到该目录下的 `bananatray.log`。

## Rotation & Cleanup

日志采用 **size-based 轮转 + 启动清理**，磁盘占用有硬上限，不会无限增长。

**轮转**：当前活跃文件 `bananatray.log` 写到 `max_bytes` 后，在一条完整日志记录的边界上轮转——
内容重命名为 `bananatray.log.1`，原 `.1` -> `.2`，以此类推，最老一份（`.{max_files}`）被删除，
随后重新打开新的空活跃文件。轮转发生在记录边界（`\n` 之后），不会截断半条日志。

**清理**：每次启动时扫描日志目录，删除序号大于 `max_files` 的残留轮转文件
（配置调小后的收敛）。运行时轮转本身已硬性封顶文件数，无需后台定时器。

**磁盘硬上限** ≈ `max_bytes × (max_files + 1)`（1 份活跃 + N 份轮转）。

| 配置项（settings.json `logging`） | 默认 | 含义 |
|---|---|---|
| `max_bytes` | 5242880 (5 MiB) | 单个日志文件大小上限，超过即轮转；`0` 禁用轮转 |
| `max_files` | 4 | 保留的轮转文件份数（不含活跃文件） |

默认磁盘上限约 25 MiB。旧 settings.json 缺少 `logging` 字段时自动回填默认值，无需迁移。
这些配置不在 UI 中暴露，需手动编辑 settings.json 调整。

> 对已存在的超大日志文件：首次写入时（`bytes_written` 已超过阈值）会立即触发一次轮转，
> 把旧内容压到 `.1`，随后按正常节奏轮转。

## Environment Variables

| 变量 | 作用 |
|------|------|
| `RUST_LOG` | 设置启动时的日志级别：`trace` / `debug` / `info` / `warn` / `error` / `off` |
| `BANANATRAY_LOG_DIR` | 覆盖日志目录 |

示例：

```bash
# 默认 info
cargo run

# 启动即开启 debug
RUST_LOG=debug cargo run

# 写入自定义目录
BANANATRAY_LOG_DIR=/tmp/bananatray-logs cargo run
```

## Runtime Diagnostics

Debug Tab 当前提供这些与日志相关的能力：

- 修改当前进程日志级别
- 打开日志目录
- 触发单 provider Debug 刷新
- 清空内存日志捕获

其中单 provider Debug 刷新会：

1. 保存当前日志级别
2. 清空并启用内存日志捕获
3. 临时把日志级别提升到 `Debug`
4. 刷新结束后恢复原级别

## Common Targets

代码中最常用于排查主流程的 target 包括：

- `app`
- `tray`
- `refresh`
- `providers`
- `settings`
- `dbus`

另外还有一些聚焦子系统的辅助 target，例如 `http`、`notification`、`single_instance`、`auto_launch`、`interactive_runner`、`providers::custom`。这些用于更细的局部诊断，不代表稳定的顶层模块边界。

排查问题时，优先按 target 过滤日志，而不是只看 message 文本。

## Issue Report

设置页 About 区域生成 issue report 时，会附带日志文件中最后 10 条 `WARN` / `ERROR` 记录。

这不是时间窗口过滤，而是“最后 N 条错误日志”摘要。为避免轮转刚发生时活跃文件几乎为空、
最近错误丢失，该摘要会跨轮转文件聚合：从最老的 `bananatray.log.{max_files}` 起依次读到
活跃文件，保留最新的 10 条。

## Practical Workflow

排查普通问题时建议按以下顺序：

1. 用默认级别复现一次
2. 如果信息不足，再用 `RUST_LOG=debug` 或 Debug Tab 提升级别
3. 查看 `providers` / `refresh` / `settings` target
4. 需要上报问题时，从 About 页生成 issue report

## Scope Limit

本文件不记录历史日志格式演化，也不承诺每个 target 的 message 文案长期稳定。
真正稳定的是：

- 日志入口
- 配置方式
- 输出位置
- 常见调试工作流
