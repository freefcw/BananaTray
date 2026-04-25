# src/runtime/effects/

`CommonEffect` 的领域化执行层。`runtime/mod.rs` 只负责 dispatch 与
`ContextEffect` 的 GPUI capability 路由；不依赖 GPUI 上下文的副作用在这里按领域执行。

## 模块边界

- `mod.rs` — `CommonEffect` 顶层穷尽分派。
- `settings.rs` — 设置持久化、自启动同步、语言与日志级别应用。
- `notification.rs` — quota、普通文本和 Debug 测试通知。
- `refresh.rs` — refresh 请求发送，以及发送失败时的前台状态降级。
- `debug.rs` — Debug 页的平台动作、日志捕获和 Debug 刷新编排。
- `newapi.rs` — NewAPI 保存 / 删除 / 加载的运行时编排。

`newapi.rs` 只编排成功 / 失败后的状态、通知和 reload；底层 YAML 文件读写继续放在
`runtime/newapi_io.rs`，纯状态回滚继续放在 `application/newapi_ops.rs`。
