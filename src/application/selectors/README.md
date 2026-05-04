# src/application/selectors/

ViewModel 选择器层，从 `AppSession` 中派生 UI 所需的 ViewModel 结构。

所有 selector 都是**纯函数**：`(&AppSession, ...) → ViewState`，不依赖 GPUI。

## 文件说明

### 核心 selector

| 文件 | 输入 | 输出 | 职责 |
|------|------|------|------|
| `tray.rs` | `&AppSession` | `HeaderViewState`, `ProviderDetailViewState`, `GlobalActionsViewState` | 弹窗面板的所有 ViewModel |
| `settings.rs` | `&AppSession` | `SettingsProvidersTabViewState` | 设置窗口 Provider 管理页 |
| `dbus_dto.rs` | `&AppSession` | `DBusQuotaSnapshot` | D-Bus JSON DTO：扁平化配额快照，供 GNOME Shell Extension 消费 |
| `debug.rs` | `&AppSession` + `DebugContext` | `DebugTabViewState` | Debug Tab（系统信息、日志捕获） |
| `issue_report.rs` | `&AppSession` + `IssueReportContext` | `IssueReport` | About 页 Issue 上报（环境 + 日志 → GitHub URL） |
| `format.rs` | `ProviderStatus`, `QuotaInfo` | `String` | 上次更新时间、配额使用详情文本 |

### 类型定义

`mod.rs` 中定义了所有 ViewModel 类型（30+ 个 struct/enum），供 selector 和 UI 共同使用。
同时，`mod.rs` 只 re-export 当前外层模块确实依赖的 selector API，避免把仅供内部组装或测试使用的类型持续暴露出去。

### 测试文件

| 测试文件 | 覆盖范围 |
|----------|---------|
| `tray_tests.rs` | header/provider detail/nav/global actions 全场景 |
| `settings_tests.rs` | provider list/detail/available providers/quota visibility |
| `debug_tests.rs` | debug info text/system info/log capture |
| `issue_report_tests.rs` | issue report 生成/URL 编码/日志截断/provider 状态 |
| `dbus_dto.rs` (内联 `#[cfg(test)]`) | StatusLevel/ConnectionStatus/ProviderId 格式化、QuotaEntry used/remaining/credit 模式、Snapshot JSON round-trip |

### dbus_dto 设计说明

`dbus_dto.rs` 定义了 D-Bus 传输用的扁平 JSON DTO（`DBusQuotaSnapshot`、`DBusProviderEntry`、`DBusQuotaEntry`、`DBusHeaderInfo`），以及对应的格式化函数（`format_status_level`、`format_connection_status`、`format_provider_id`）。

**放在 `application/selectors/` 而非 `dbus/` 的原因**：DTO 类型和格式化逻辑不依赖 GPUI/zbus，可在任何平台编译和测试。`dbus/` 模块受 `cfg(target_os = "linux")` 门控，其内部代码无法在 macOS/Windows 上运行测试。`dbus/serde_types.rs` 仅做 re-export 保持接口不变。

## 设计原则

- **单一职责**：每个 selector 文件只负责一个 UI 区域的 ViewModel 派生
- **共享格式化**：`format.rs` 集中管理所有格式化/国际化逻辑，避免 selector 间重复
- **可测试性**：纯函数，入参为 `&AppSession`（可在测试中自由构造），无 I/O 依赖
- `DebugContext` / `IssueReportContext` 是需要运行时信息（日志路径、环境变量）的 selector 入参，副作用数据由 `runtime` 诊断上下文 collector 一次性收集后注入
