# "window not found" 错误修复记录

> 这是历史问题分析文档，不是当前窗口实现的实时设计稿。
> 文中保留了当时的旧路径、旧调用链和阶段性修复步骤；对于当前 popup / settings 生命周期边界，请以 `docs/architecture.md` 和现行代码为准。

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

### 2.3 确定根因（历史版本）

**根因（当时）：tray popup 窗口被 `remove_window()` 关闭后，`AppState.view_entity` 仍然持有指向该窗口 view 的 `WeakEntity<AppView>` 引用。**

> 后续重构说明：当前代码已进一步演进，`view_entity` 已从 `AppState` 中移除，popup view 的弱引用改由 `ui` 模块内部持有，并通过 `runtime/ui_hooks.rs` 与 `runtime` 交互。本文档保留的是该问题发生时的分析与修复过程。

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

### 2.4 受影响的关闭路径（历史版本）

排查所有调用 `remove_window()` 的位置：

| 路径 | 文件 | 触发方式 |
|------|------|----------|
| `TrayController::toggle_provider` | main.rs | 左键点击 tray icon 切换 |
| `TrayController::show_settings` | main.rs | 右键点击 tray icon |
| auto-hide observer | main.rs | 窗口失焦自动关闭 |
| settings icon button | app/mod.rs | popup 内点击设置图标 |
| "Open Settings" button | provider_panel.rs | provider 面板内打开设置 |

5 条路径，修复前**无一清理 `view_entity`**。

## 3. 解决方案（该次修复）

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

> 当前状态补充：这套修复之后，代码又继续重构，关闭 popup 的清理职责已通过 UI hook 与 `TrayController` 协同完成，而不是继续把 `view_entity` 保存在 `AppState` 上。

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

---

## 7. 2026-04-02 后续排查：残留 "window not found" 与 RefCell 崩溃

### 7.1 残留 "window not found" 日志

#### 表征

经过第 3 节的修复后，应用日志中仍偶发 `[ERROR]              window not found`（空 target），典型时序：

```
23:53:40.343 [INFO] tray         tray popup opened successfully
23:53:46.387 [INFO] tray         auto-hide closing inactive tray popup
23:54:03.264 [ERROR]              window not found
23:54:03.395 [ERROR]              window not found
23:54:03.395 [ERROR]              window not found
```

关键特征：
- 出现在 auto-hide 关闭窗口 **17 秒后**，不再是密集帧循环
- 日志 target 为空字符串，非应用代码产生

#### 根因

`window.remove_window()` 只是设置 `self.removed = true`（标记位），**真正的窗口移除发生在当次 `update_window_id()` 返回时**（检测 `window.removed` 后从 `cx.windows` slab 中移除）。

但 GPUI 在窗口创建时注册了多个平台回调，特别是 **macOS display link 帧回调** (`on_request_frame`)，这些回调持有 `WindowHandle` 的拷贝：

```rust
// adabraka-gpui-0.5.1/src/window.rs (简化)
platform_window.on_request_frame(Box::new({
    let mut cx = cx.to_async();
    move |_| {
        handle
            .update(&mut cx, |_, window, cx| {
                window.draw(cx);   // 帧绘制
                window.present();  // 显示
            })
            .log_err();  // ← 窗口移除后，update 返回 Err → log_err 打印 ERROR
        handle
            .update(&mut cx, |_, window, _| {
                window.complete_frame();
            })
            .log_err();  // ← 同上
    }
}));
```

窗口从 `cx.windows` 中移除后，display link 可能仍有排队的帧回调等待执行（macOS 帧调度与 GPUI 事件循环异步），这些回调调用 `handle.update()` 时发现窗口不存在，通过 `.log_err()` 输出错误。

#### 为什么 target 为空

`.log_err()` 来自 `adabraka_util` crate 的 `ResultExt` trait，其 `log_error_with_caller` 实现通过 `caller.file().split_once("crates/")` 提取 log target。但通过 cargo registry 安装的 crate 路径为 `~/.cargo/registry/src/.../adabraka-gpui-0.5.1/src/window.rs`，不含 `crates/` 前缀，导致 target 解析为空字符串 `""`。

#### 处理

这是 GPUI 框架内部的 display link 生命周期管理问题，**外部代码无法干预**，且**无害**（仅日志噪音）。

通过 fern 日志过滤器消除噪音：

```rust
// src/logging.rs
fern::Dispatch::new()
    .level(level)
    // ...
    .filter(|metadata| {
        // 空 target 的 ERROR 来自 GPUI 内部（registry crate 路径无 "crates/" 前缀
        // 导致 target 为空），降级过滤
        !(metadata.target().is_empty() && metadata.level() == log::Level::Error)
    })
```

### 7.2 RefCell already borrowed 崩溃

#### 表征

从 tray popup 打开设置窗口时 panic 崩溃：

```
00:00:39.316 [INFO] settings     existing settings window found, attempting to activate it
00:00:39.316 [INFO] settings     existing handle is stale, clearing
00:00:39.320 [ERROR] bananatray::panic panic at src/app/settings_window/window_mgr.rs:163:
    RefCell already borrowed
```

#### 根因

`open_settings_window()` 中的 `SETTINGS_WINDOW` thread_local 存在 **RefCell 双重借用**：

```rust
// 修复前 — window_mgr.rs
let activated_existing = SETTINGS_WINDOW.with(|slot| {
    if let Some(handle) = slot.borrow().as_ref() {  // ← borrow() 开始，生命周期覆盖整个闭包
        // ...
        let ok = handle.update(cx, ...).is_ok();
        if !ok {
            *slot.borrow_mut() = None;  // ← 💥 borrow() 仍存活时调用 borrow_mut()
        }
        ok
    } else {
        false
    }
});
```

`slot.borrow()` 返回的 `Ref` 的生命周期延伸到 `if let Some(handle)` 整个分支，而在该分支末尾的 `slot.borrow_mut()` 尝试可变借用同一个 `RefCell`，违反借用规则导致 panic。

#### 解决方案

先将 `WindowHandle` copy 出来（`WindowHandle` 实现了 `Copy`），释放 `slot` 的借用后再操作：

```rust
// 修复后
let existing_handle = SETTINGS_WINDOW.with(|slot| *slot.borrow());  // Copy 出来，borrow 立即释放
let activated_existing = if let Some(handle) = existing_handle {
    // ... 操作 handle，需要清理时单独调用：
    SETTINGS_WINDOW.with(|slot| *slot.borrow_mut() = None);  // 安全，无冲突借用
    // ...
} else {
    false
};
```

### 7.3 两个问题的关系

| 维度 | "window not found" | RefCell 崩溃 |
|------|-------------------|-------------|
| 来源 | GPUI 框架内部 | 应用代码 `window_mgr.rs` |
| 严重性 | 无害（日志噪音） | 致命（panic → abort） |
| 共享前置条件 | 窗口被关闭/handle 变 stale | 同左 |
| 直接因果关系 | ❌ 无 | ❌ 无 |

两者**没有直接因果关系**，但共享同一个前置条件：窗口被 auto-hide 关闭后 handle 变为 stale。它们是同一场景下的两个独立缺陷。

### 7.4 变更文件清单

| 文件 | 变更内容 |
|------|----------|
| `src/logging.rs` | 新增 `.filter()` 过滤空 target 的 ERROR 日志 |
| `src/app/settings_window/window_mgr.rs` | 修复 `SETTINGS_WINDOW` RefCell 双重借用 |

---

**最后更新**：2026-04-02
