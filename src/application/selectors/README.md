# src/application/selectors/

ViewModel 选择器层，从 `AppSession` 中派生 UI 所需的 ViewModel 结构。

所有 selector 都是**纯函数**：`(&AppSession, ...) → ViewState`，不依赖 GPUI。

## 文件说明

### 核心 selector

| 文件 | 输入 | 输出 | 职责 |
|------|------|------|------|
| `tray.rs` | `&AppSession` | `HeaderViewState`, `ProviderDetailViewState`, `GlobalActionsViewState` | 弹窗面板的所有 ViewModel |
| `settings.rs` | `&AppSession` | `SettingsProvidersTabViewState` | 设置窗口 Provider 管理页 |
| `debug.rs` | `&AppSession` + `DebugContext` | `DebugTabViewState` | Debug Tab（系统信息、日志捕获） |
| `format.rs` | 各种原始数据 | `String` | 共享格式化函数（时间、百分比、quota 文本等） |

### 类型定义

`mod.rs` 中定义了所有 ViewModel 类型（30+ 个 struct/enum），供 selector 和 UI 共同使用。

### 测试文件

| 测试文件 | 覆盖范围 |
|----------|---------|
| `tray_tests.rs` | header/provider detail/nav/global actions 全场景 |
| `settings_tests.rs` | provider list/detail/available providers/quota visibility |
| `debug_tests.rs` | debug info text/system info/log capture |

## 设计原则

- **单一职责**：每个 selector 文件只负责一个 UI 区域的 ViewModel 派生
- **共享格式化**：`format.rs` 集中管理所有格式化/国际化逻辑，避免 selector 间重复
- **可测试性**：纯函数，入参为 `&AppSession`（可在测试中自由构造），无 I/O 依赖
- `DebugContext` 是唯一需要运行时信息（日志路径、环境变量）的 selector 入参，这些 I/O 在构造 `DebugContext` 时一次性完成
