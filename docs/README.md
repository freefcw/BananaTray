# Docs

`docs/` 只记录 BananaTray 的稳定边界、支持的工作流和仍然有参考价值的专题说明。

低层实现细节会持续移动，因此这里不再把文件树、函数调用链、测试数量这类信息当成长期契约。
对于正在演进的实现，当前代码和测试才是最终事实源。

## 文档约定

- 权威文档只描述稳定职责、行为契约和对外工作流。
- 文件级实现细节优先放在对应模块的 `README.md`，并以代码为准。
- 重复信息采用"单一事实源 + 交叉引用"策略，避免多处维护同一事实。
- 历史复盘、事故记录、重构审查会保留，但默认**不**承诺和当前代码逐行同步。

## 当前权威文档

- `architecture.md`
  - 系统边界、状态流、运行时分层、持久化与测试约束。
- `providers.md`
  - 内置 / 自定义 provider 模型、扩展边界、错误与设置能力约定。
- `custom-provider.md`
  - YAML 自定义 provider 的用户指南、Schema 摘要和排障建议。
- `refresh-strategy.md`
  - 刷新触发源、调度规则、并发执行和 reload 语义。
- `logging.md`
  - 日志级别、日志文件位置、调试相关入口。
- `release.md`
  - GitHub Release 的 tag 触发、自动草稿、产物和人工发布清单。
- `gnome-shell-extension-development.md`
  - GNOME Shell Extension 的开发、nested Shell 调试、mock/真实 daemon、D-Bus 排查和验证清单。

## Linux 专属模块文档

这些模块仅在 Linux + `app` feature 下编译，详细接口和架构见对应 `README.md`：

- `src/dbus/README.md`
  - D-Bus 服务架构、线程模型、接口契约（方法/信号/属性）、JSON 快照格式。
- `src/platform/README.md`
  - 平台适配层的 app-only / lib-safe 边界、稳定路径和 OS 集成约束。
- `gnome-shell-extension/README.md`
  - GNOME Shell Extension 安装、使用说明、D-Bus 通信流程、组件架构、排障指南。

## 参考文档

这些文档仍然描述当前设计思路，但它们是专题参考，不是总体架构契约：

- `provider-blueprints.md`
  - 新增 / 重构 provider 时可复用的设计模式。
- `antigravity-api.md`
  - Antigravity / Windsurf 共享实现的专题说明。

## 示例

- `examples/`
  - 自定义 provider YAML 示例（NewAPI、HTTP、CLI、Placeholder 等）。

## 历史文档

历史材料统一放在 `archive/` 下；其中的旧路径、旧测试数量、旧模块名不应被当成当前事实：

- `archive/gnome-shell-extension-plan.md` — GNOME Extension 预研方案与实现对照（使命已完成）
- `archive/provider-compare-analysis.md` — BananaTray / ClaudeBar / CodexBar 三仓库 provider 横向对比（2026-05-05 快照）
- `archive/code-review-solid-clean.md` — SOLID & Clean Code 深度分析
- `archive/gpui-sigbus-bug.md` — GPUI SIGBUS bug 排查记录
- `archive/window-not-found-fix.md` — Window Not Found 修复记录
- `archive/lessons-gpui-height-calibration.md` — GPUI 弹窗高度校准经验
- `archive/ui_layout_troubleshooting.md` — UI 布局排查过程
- `archive/custom-provider-plan.md` — 自定义 provider 方案设计
- `archive/review-20260405.md` — 2026-04-05 代码审查
- `archive/review-20260416-followup-issues.md` — 2026-04-16 审查后续跟进
- `archive/roadmap-directory-restructure.md` — 目录重组路线图
- `archive/provider/provider-refactor-retrospective.md` — Provider 重构回顾
- `archive/provider/refactor.md` — Provider 重构方案
- `archive/app/analytics.md` — 应用分析功能设计

## 建议阅读顺序

1. `architecture.md`
2. `providers.md`
3. 与当前任务直接相关的专题文档
4. 需要时再查 `src/*/README.md` 和具体代码

## 维护建议

- 改架构边界时，先更新 `architecture.md`。
- 改 provider 契约或扩展方式时，更新 `providers.md`；如果涉及 YAML，再同步 `custom-provider.md`。
- 如果某段说明只能靠具体文件路径或行级细节才能成立，优先把它移出权威文档，改为专题说明或直接删掉。
- 重复信息不要复制粘贴，用交叉引用指向单一事实源。
