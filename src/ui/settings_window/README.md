# src/ui/settings_window/

设置窗口 UI 模块，独立窗口 + Tab 导航 + 双栏 Provider 管理。

## 文件说明

### 窗口管理

| 文件 | 职责 |
|------|------|
| `mod.rs` | **`SettingsView`** 主视图 — 头部、Tab 导航栏、内容区路由。含 `NewApiFormInputs` 表单状态管理 |
| `window_mgr.rs` | 窗口生命周期管理 — `schedule_open_settings_window()`：延迟到下一帧创建窗口（避免 RefCell 重入）；多显示器定位 |
| `components.rs` | 设置页共享组件（section title、description text 等） |

### Tab 内容页

| 文件 | Tab | 内容 |
|------|-----|------|
| `general_tab.rs` | General | 系统行为设置：自启动、自动隐藏、刷新间隔、全局热键 |
| `display_tab.rs` | Display | 外观设置：主题、语言、托盘图标样式、配额显示模式、UI 开关 |
| `about_tab.rs` | About | 版本信息、系统信息、开源许可、贡献者、问题上报（GitHub Issue） |
| `debug_tab.rs` | Debug | 调试控制台：日志捕获、单 Provider 刷新、通知测试、系统诊断文本 |

### Provider 管理（双栏布局）

`providers/` 子目录实现设置窗口的 **Providers** Tab：

| 文件 | 职责 |
|------|------|
| `providers/mod.rs` | 入口 — 双栏布局组装（sidebar + divider + right panel 三态切换） |
| `providers/sidebar.rs` | 左侧 Sidebar — Provider 列表（拖拽排序、添加/删除按钮） |
| `providers/detail.rs` | 右侧详情 — Provider 信息/状态/配额/配额可见性/启用开关/Copilot Token 输入 |
| `providers/picker.rs` | 添加面板 — 可选 Provider 列表（从 sidebar 中排除已添加的） |
| `providers/newapi_form.rs` | NewAPI 表单 — 自定义 Provider 快速添加/编辑表单（name, url, cookie, user_id, divisor） |

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
```

## 约束

- 设置窗口和托盘弹窗是**不同的 GPUI 窗口**，可同时存在
- `window_mgr.rs` 使用 `cx.spawn()` 延迟创建窗口，避免在 effect handler 中直接创建导致 RefCell 重入
- `NewApiFormInputs` 使用 adabraka-ui 的 `InputState`（单行输入）和 `TextareaState`（Cookie 等长文本多行编辑）
