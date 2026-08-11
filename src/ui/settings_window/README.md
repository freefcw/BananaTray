# src/ui/settings_window/

设置窗口 UI 模块，独立窗口 + Tab 导航 + 双栏 Provider 管理。

## 文件说明

### 窗口管理

| 文件 | 职责 |
|------|------|
| `mod.rs` | **`SettingsView`** 主视图 + `build_settings_view()` 工厂 — 头部、Tab 导航栏、内容区路由；含 `TokenInputDraft`、`NewApiFormInputs`、`ScriptProviderFormInputs` 表单状态和 Debug 诊断快照管理；表单缓存按 modal identity 驱动重建，shell hook 由 `bootstrap` 注册 |
| `components.rs` | 设置页共享组件（section title、description text 等） |

### Tab 内容页

| 文件 | Tab | 内容 |
|------|-----|------|
| `general_tab.rs` | General | 系统行为与通知设置：自启动、全局热键、刷新间隔、配额通知、提示音 |
| `display_tab.rs` | Display | 外观设置：主题、语言、托盘图标样式、配额显示模式、UI 开关 |
| `about_tab.rs` | About | 版本信息、系统信息、开源许可、贡献者、问题上报（GitHub Issue） |
| `debug_tab.rs` | Debug | 调试控制台：日志捕获、单 Provider 刷新、通知测试、后台采集并可手动刷新的系统诊断快照 |

### Provider 管理（双栏布局）

`providers/` 子目录实现设置窗口的 **Providers** Tab：

| 文件 | 职责 |
|------|------|
| `providers/mod.rs` | 入口 — 双栏布局组装（sidebar + divider + right panel 三态切换），右侧表单 identity 由 selector 显式下发 |
| `providers/shared.rs` | Provider 表单/按钮共享基元（字段标签、输入框、只读字段、确认/取消按钮） |
| `providers/sidebar.rs` | 左侧 Sidebar — Provider 列表（拖拽排序、添加/删除按钮） |
| `providers/detail/` | 右侧详情模块 — shell、section renderer、配额可见性和 editable-provider actions；模块契约见 `providers/detail/README.md` |
| `providers/picker.rs` | 添加面板 — 可选 Provider 列表（从 sidebar 中排除已添加的） |
| `providers/token_input_panel.rs` | Token 输入面板 — 通用 Provider token 设置 UI；编辑草稿由 `SettingsView::begin_token_input()` 创建并在会话内复用 |
| `providers/newapi_form.rs` | NewAPI 表单 — 自定义 Provider 快速添加/编辑表单（name, url, cookie, user_id, divisor） |
| `providers/script_provider_form.rs` | 自定义脚本表单 — 编辑 provider 名称、解释器、超时和脚本源码；Run Test 后台解析 stdout JSON，保存后生成脚本文件与 `source.type: cli` YAML |

## 窗口交互流程

```
SettingsView::render()
  ├── render_header()      — 图标 + "Settings" + ✕ 关闭按钮
  ├── render_tab_bar()     — 水平 pill 导航（General / Providers / Display / About / Debug?）
  └── content area         — 按 active_tab 路由到对应 Tab 渲染
        └── Providers Tab
              ├── sidebar          — 已添加的 Provider 列表
              ├── divider          — 竖线分隔
              └── right panel      — 三态切换：
                    ├── detail     — Provider 配置详情
                    ├── picker     — 添加新 Provider 选择
                    └── newapi     — NewAPI 自定义 Provider 表单
                    └── script     — 自定义脚本 Provider 表单
```

## 约束

- 设置窗口和托盘弹窗是**不同的 GPUI 窗口**，可同时存在
- 设置窗口的异步调度与多显示器复用逻辑已迁至 `bootstrap::schedule_open_settings_window()`
- Token 输入框使用 view-local `TokenInputDraft` 复用 `InputState`，进入编辑时创建草稿，保存 / 取消 / 离开当前 provider 入口时清理；输入容器必须注册 `key_context("Input")` 才能接收标准编辑动作
- General Tab 的全局热键区域使用 view-local `HotkeyInputState` 做键捕获，`SettingsView` 额外维护一个已同步快照，避免成功保存前覆盖用户正在录制的候选值
- 真正的热键预检、重绑与错误回填仍由 `AppAction::SaveGlobalHotkey` → runtime effect 完成；设置页只会在当前候选值仍等于上次失败候选时显示 runtime 错误，避免把旧失败提示错误地挂到新录制结果上
- macOS 下该保存流现在会落到系统级 `RegisterEventHotKey` 注册，而不是旧的 `NSEvent` monitor 监听
- `NewApiFormInputs` 使用 adabraka-ui 的 `InputState`（单行输入）和 `TextareaState`（Cookie 等长文本多行编辑）；右侧面板 selector 会显式传入当前 form identity，同一 identity 复用输入实体，不同 identity 重建，避免跨 provider 串用旧草稿
- `ScriptProviderFormInputs` 同样使用 `InputState` + `TextareaState`，provider id 由名称生成并只读展示；编辑模式保留原始 YAML / 脚本文件名，避免保存时改名造成残留文件；缓存重建规则与 NewAPI 表单一致
- Debug 环境诊断在进入 Tab 或点击刷新按钮时由后台执行器采集；`render_debug_tab()` 只能读取缓存，不得执行文件 metadata、外部命令等阻塞操作
