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

`newapi.rs` 的前台 `run()` 只把 YAML 保存 / 删除 / 编辑态加载放入持久 blocking 队列；worker 侧 `execute()` 才执行 I/O，并把结果先写入可靠 ledger，再通过前台唤醒转换成 `NewApi*Finished` action。关闭入队端后 worker 在共同退出截止时间内 drain/join；超时 detach 不保证未完成事务结算，退出时已收到但尚未消费的 ledger action 由 shutdown 同步结算。状态回滚、通知 key 选择、render 和 provider reload 都在 `application/reducer/newapi.rs` 中统一声明；底层 YAML 文件读写与 provider 编辑态加载统一放在 `providers::custom::api`。

`script_provider.rs` 将 CRUD 与 Run Test 分开：保存 / 删除 / 编辑加载进入持久串行 `custom-provider-io-worker`，保存成功后的 deferred settings flush 也在该 worker 上等待；Run Test 进入独立、可取消的串行 `script-test-worker`。CRUD 完成 action 先进入可靠 ledger，再由前台 pump 结算；入队失败则在当前 dispatch 中直接返回后续 action。Run Test 结果通过其独立 action 通道回到前台 reducer。成功后的 provider reload、partial delete 通知和失败回滚由 `application/reducer/script_provider.rs` 处理。
