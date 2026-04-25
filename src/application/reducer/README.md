# src/application/reducer/

Reducer 子模块目录。这里承载 `AppAction` 到 `AppSession` 状态变更和 `AppEffect`
声明的纯逻辑实现；顶层 `src/application/reducer.rs` 只负责按 action 分发。

## 模块职责

| 文件 | 职责 |
|------|------|
| `settings.rs` | 导航、设置窗口通用 UI 状态、`SettingChange`、全局热键提交、弹窗可见性 |
| `provider_sidebar.rs` | Provider 开关、设置页 Provider 选择、token 编辑、sidebar 增删和排序 |
| `refresh.rs` | 手动刷新、刷新事件处理、Provider 热重载、热重载后的悬空引用清理 |
| `newapi.rs` | NewAPI 新增 / 编辑 / 删除表单流，以及对应 effect 发射 |
| `debug.rs` | Debug Tab 操作、调试刷新、日志目录 / 剪贴板 / 调试通知相关 action |
| `shared.rs` | 跨子 reducer 共享的纯 helper，如配置同步请求、刷新能力判断、动态图标同步 |

## 数据流

```
AppAction
  -> reducer.rs::reduce()
    -> reducer/<domain>.rs
      -> mutate AppSession
      -> append AppEffect
```

子 reducer 不执行 I/O，不依赖 GPUI，也不直接调用 runtime。需要外部行为时只追加
`CommonEffect` 或 `ContextEffect`，由 runtime 层统一执行。

## 约束

- 保持纯函数边界：给定 `AppSession` 和 `AppAction`，只产生确定的状态变更和 effect 列表。
- 新 action 优先按业务归属放入对应子 reducer；只有跨领域的无状态 helper 放入 `shared.rs`。
- 不从这里导入 `providers/` 或 GPUI 类型，避免破坏 application 层的测试边界。
- 测试仍集中挂在顶层 `reducer.rs` 的 `reducer_tests.rs`，覆盖跨子 reducer 的行为契约。
