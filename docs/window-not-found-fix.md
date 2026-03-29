# "window not found" 错误修复记录

> 日期：2026-03-30

## 1. 现象

应用运行时日志出现连续的 `[ERROR] window not found`，发生在从 tray popup 切换到 settings 窗口的过程中：

```
2026-03-30 00:19:39.911 [INFO] tray         tray popup opened successfully
2026-03-30 00:19:41.171 [INFO] settings     scheduled async settings window open (display: Some(DisplayId(2)))
2026-03-30 00:19:41.172 [ERROR]              window not found
2026-03-30 00:19:41.184 [INFO] settings     requested settings window
2026-03-30 00:19:41.247 [INFO] settings     constructing settings view
2026-03-30 00:19:41.268 [INFO] settings     opened new settings window
2026-03-30 00:19:41.271 [INFO] settings     requested app/window activation for settings window
2026-03-30 00:19:41.339 [ERROR]              window not found
2026-03-30 00:19:41.357 [ERROR]              window not found
2026-03-30 00:19:41.374 [ERROR]              window not found
    ... (持续输出)
```

关键特征：
- 错误没有 target 标识（非应用代码打印）
- 错误以约 17ms 间隔重复出现，呈现帧循环特征
- 发生在 tray popup 关闭后、settings 窗口打开前后

## 2. 分析过程

### 2.1 定位错误来源

在项目代码中搜索 `"window not found"` 无结果——这不是我们的日志。

在 GPUI 框架源码 (`adabraka-gpui-0.5.1`) 中找到三处：

| 文件 | 行号 | 上下文 |
|------|------|--------|
| `src/app.rs` | 1682 | `update_window_id()` — 通过 WindowId 更新窗口时窗口不存在 |
| `src/app.rs` | 2495 | `read_window()` — 读取窗口时窗口不存在 |
| `src/window.rs` | 4793 | 窗口相关操作 |

核心是 `update_window_id` 方法：

```rust
fn update_window_id<T, F>(&mut self, id: WindowId, update: F) -> Result<T> {
    self.update(|cx| {
        let mut window = cx.windows.get_mut(id)?.take()?;
        // ... 执行 update ...
        Some(result)
    })
    .context("window not found")  // ← 错误来源
}
```

当 `cx.windows` slab 中找不到指定 WindowId 的条目时，返回 `None`，被 `.context()` 包装为 "window not found" 错误。

### 2.2 追踪触发链路

应用架构中，后台刷新协调器通过事件泵通知 UI 刷新：

```rust
// 事件泵（main.rs）
while let Ok(event) = event_rx.recv().await {
    let view_entity = {
        let mut s = state.borrow_mut();
        s.apply_refresh_event(event);
        s.view_entity.clone()  // WeakEntity<AppView>
    };
    if let Some(entity) = view_entity {
        let _ = entity.update(&mut pump_cx, |_, cx| {
            cx.notify();  // ← 关键调用
        });
    }
}
```

`cx.notify()` 的内部流程：

```
cx.notify(entity_id)
  → App::notify()
    → 查找 window_invalidators_by_entity[entity_id]
      → invalidator.invalidate_view()
        → 标记 dirty，推入 Effect::Notify
          → flush_effects()
            → update_window_id(window_id, ...)
              → windows.get_mut(id) 返回 None
                → "window not found" ✗
```

### 2.3 确定根因

**根因：tray popup 窗口被 `remove_window()` 关闭后，`AppState.view_entity` 仍然持有指向该窗口 view 的 `WeakEntity<AppView>` 引用。**

时序图：

```
T+0s    tray popup 打开，view_entity 指向 popup 的 AppView
T+1.2s  用户右键 → show_settings() → remove_window() 关闭 popup
        ⚠ view_entity 未清除，仍指向已销毁窗口的 entity
T+1.2s  schedule_open_settings_window → 10ms 延迟后打开 settings
T+1.2s  后台刷新事件到达 → 事件泵取 view_entity → cx.notify()
        → GPUI 尝试 invalidate 已不存在的窗口 → "window not found"
T+1.2s+ 每个后续刷新事件重复触发 → 连续错误
```

`WeakEntity` 能成功 `upgrade()` 是因为 entity 本身尚未被 GPUI 的 entity_map 释放（强引用仍存在于 GPUI 内部的 observer 闭包中），但它关联的窗口已从 `windows` slab 中移除。这是 entity 生命周期与窗口生命周期不一致造成的间隙。

### 2.4 受影响的关闭路径

排查所有调用 `remove_window()` 的位置：

| 路径 | 文件 | 触发方式 |
|------|------|----------|
| `TrayController::toggle_provider` | main.rs | 左键点击 tray icon 切换 |
| `TrayController::show_settings` | main.rs | 右键点击 tray icon |
| auto-hide observer | main.rs | 窗口失焦自动关闭 |
| settings icon button | app/mod.rs | popup 内点击设置图标 |
| "Open Settings" button | provider_panel.rs | provider 面板内打开设置 |

5 条路径，修复前**无一清理 `view_entity`**。

## 3. 解决方案

### 设计原则

- **单一职责**：关闭窗口的逻辑集中到一个方法，而非散落在每个调用点
- **纵深防御**：即时清理 + Drop 安全网，确保无遗漏
- **最小改动**：不修改 GPUI 框架，在应用层解决

### 3.1 提取 `TrayController::close_popup()` 方法

将"清除 view_entity + 关闭窗口"封装为原子操作：

```rust
/// Close the tray popup window and clear the view entity reference.
/// Returns the display ID the popup was on, if available.
fn close_popup(&mut self, cx: &mut App) -> Option<DisplayId> {
    let window = self.window.take()?;
    self.state.borrow_mut().view_entity = None;
    let mut display_id = None;
    let _ = window.update(cx, |_, window, cx| {
        display_id = window.display(cx).map(|d| d.id());
        window.remove_window();
    });
    display_id
}
```

`toggle_provider` 和 `show_settings` 均改为调用此方法，消除重复代码。

### 3.2 UI 内部关闭路径 — 即时清理

popup 内部的按钮回调（settings icon、"Open Settings" button）持有 `Rc<RefCell<AppState>>`，在 `remove_window()` 前清除：

```rust
.on_mouse_down(MouseButton::Left, move |_, window, cx| {
    state.borrow_mut().view_entity = None;  // ← 新增
    window.remove_window();
    // ...
})
```

auto-hide observer 同理：

```rust
if should_auto_hide && !window.is_window_active() {
    auto_hide_state.borrow_mut().view_entity = None;  // ← 新增
    window.remove_window();
}
```

### 3.3 `AppView::Drop` — 安全网

为 `AppView` 实现 `Drop`，确保即使未来新增的关闭路径遗漏了清理，`view_entity` 也最终会被清除：

```rust
impl Drop for AppView {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.try_borrow_mut() {
            state.view_entity = None;
        }
    }
}
```

使用 `try_borrow_mut` 而非 `borrow_mut`，防止 GPUI 内部 effect flush 期间 `RefCell` 已被借用时发生 panic。

### 3.4 防御层次总结

```
Layer 1: close_popup()        — controller 发起的关闭（toggle/settings）
Layer 2: 回调内即时清理        — UI 内部发起的关闭（button/auto-hide）
Layer 3: AppView::Drop        — 兜底安全网（任何未覆盖的路径）
```

## 4. 变更文件清单

| 文件 | 变更内容 |
|------|----------|
| `src/main.rs` | 新增 `close_popup()` 方法；重构 `toggle_provider`/`show_settings` 使用它；auto-hide 回调增加清理 |
| `src/app/mod.rs` | settings icon button 回调增加清理；新增 `AppView::Drop` 实现 |
| `src/app/provider_panel.rs` | "Open Settings" button 回调增加清理 |

## 5. 测试验证

- `cargo check` — 零错误零警告
- `cargo test` — 全部 120 个测试通过（55 lib + 55 bin + 10 integration）
- 窗口生命周期管理涉及 GPUI 平台层，无法编写纯单元测试，需运行时验证

## 6. 经验总结

1. **"window not found" 是 GPUI 框架内部的错误日志**，定位时需要到依赖的 crate 源码中搜索，而非仅搜索项目代码。
2. **`WeakEntity` 的生命周期与窗口生命周期不同步**是 GPUI 的一个陷阱。entity 可以在窗口被 remove 后仍然存活（被 observer 闭包强引用），对其调用 `cx.notify()` 会触发对已销毁窗口的 invalidation。
3. **所有"关闭窗口"的路径都必须清理关联状态**，提取统一方法是避免遗漏的最佳实践。
