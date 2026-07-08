# src/runtime/effects/

`CommonEffect` 的领域化执行层。`runtime/mod.rs` 只负责 dispatch 与
`ContextEffect` 的 GPUI capability 路由；不依赖 GPUI 上下文的副作用在这里按领域执行。

## 模块边界

- `mod.rs` — `CommonEffect` 顶层穷尽分派。
- `settings.rs` — 设置持久化、自启动同步、语言与日志级别应用。
- `notification.rs` — quota、普通文本和 Debug 测试通知。
- `refresh.rs` — refresh 请求发送；发送失败时返回后续 `RefreshEventReceived` action，由 reducer 统一应用失败状态和 render effect。
- `debug.rs` — Debug 页的平台动作、日志捕获和 Debug 刷新编排。
- `newapi.rs` — NewAPI 保存 / 删除 / 加载的运行时编排。
- `script_provider.rs` — 自定义脚本 Provider 的 Run Test、脚本 + YAML 保存、删除和编辑加载编排。

`newapi.rs` 只执行 YAML 保存 / 删除 / 编辑态加载等 I/O，并把结果转换成 `NewApi*Finished` 后续 action。状态回滚、通知 key 选择、render 和 provider reload 都在 `application/reducer/newapi.rs` 中统一声明；底层 YAML 文件读写与 provider 编辑态加载统一放在 `providers::custom::api`。

`script_provider.rs` 遵守相同边界：Run Test 只把请求发送到脚本测试事件泵，后台执行完成或排队失败都通过 `ScriptProviderTestFinished` 回到前台 reducer；保存 / 删除 / 编辑加载都通过 `providers::custom::api`，并转换成 `ScriptProvider*Finished` action。成功后的 provider reload、partial delete 通知和失败回滚由 `application/reducer/script_provider.rs` 处理。
