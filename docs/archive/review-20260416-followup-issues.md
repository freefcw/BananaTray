# BananaTray 审查后续跟进清单（2026-04-16）

> 来源：基于 `review-20260416-deep-audit.md` 的问题拆解版，面向后续 issue / project 卡片跟进。
>
> 本文目标不是重复审查结论，而是把“还要做什么”整理成可执行列表。

## 当前基线

执行 `cargo update` 后，当前仓库的默认构建链已经恢复：

- `cargo check` 通过
- `cargo test --lib` 通过，`799 passed`
- `cargo clippy --lib -- -D warnings` 通过
- `cargo check --release` 通过

但以下边界问题仍然存在：

- `cargo check --no-default-features` 失败
- `cargo clippy --lib --no-default-features -- -D warnings` 失败
- `ProviderManager` 生命周期仍有双所有权风险
- `ProviderStatus` 仍混入展示/i18n 文本
- 文档与实现仍有明显漂移

## 跟进原则

1. 先处理“会继续制造错误判断”的契约问题，再做纯清理。
2. 先收口边界，再回收 workaround。
3. 所有与依赖、feature、架构边界有关的修复，都要同步更新对应文档。

## 建议拆分的 Issue 列表

| ID | 优先级 | 标题 | 建议 Owner | 阻塞关系 |
| --- | --- | --- | --- | --- |
| F1 | P1 | 收口 `app` feature 契约，明确是否正式支持 `--no-default-features` | runtime / app shell | 阻塞文档对齐 |
| F2 | P1 | 统一 `ProviderManager` 的所有权，消除 reload 后的前后台分叉 | runtime / refresh | 影响 token UI、provider reload |
| F3 | P1 | 将本地化展示文本从 `ProviderStatus` 中拆出 | application / providers / ui | 影响语言切换与状态模型 |
| F4 | P1 | 为 HTTP client 引入结构化错误边界 | providers | 为错误分类清理打基础 |
| F5 | P2 | 做一轮文档对齐专项，修正当前错误入口和过时描述 | docs / module owners | 依赖 F1 的结论 |
| F6 | P2 | 清理 `src/providers/antigravity/` 下未参与模块图的旧实现文件 | providers | 独立可做 |
| F7 | P2 | 收窄 blanket `allow(...)` 与 dead-code suppress 范围 | platform / ui / providers | 最好在 F1 后做 |
| F8 | P2 | 重新定义 placeholder provider 的能力层级和 UI 语义 | providers / product model | 独立可做 |
| F9 | P3 | 建立 workaround 台账和回收条件 | runtime / platform / tray | 最好在 F2/F3 后做 |

## 详细拆解

### F1. 收口 `app` feature 契约，明确是否正式支持 `--no-default-features`

**为什么要做**

当前最大的“认知错误源”不是默认构建失败，而是仓库对外仍然暗示 `app` feature 可以关闭，但真实实现没有把 bin 路径一起收口。结果是：

- `cargo test --lib --no-default-features` 通过
- `cargo check --no-default-features` 失败
- `cargo clippy --lib --no-default-features -- -D warnings` 失败

这会让开发者误以为 feature 支持已经完整。

**证据入口**

- `src/main.rs:21`
- `src/main.rs:47-49`
- `src/platform/mod.rs:27-33`
- `src/application/newapi_ops.rs:15`
- `src/application/newapi_ops.rs:33`
- `src/application/newapi_ops.rs:57`
- `docs/gpui-sigbus-bug.md:176`
- `docs/gpui-sigbus-bug.md:228`

**建议动作**

- 先做一个明确决策：
  - 路线 A：正式支持 `--no-default-features`
  - 路线 B：不再承诺该能力，只保留 `lib` 层局部测试用途
- 如果走路线 A：
  - 让 `main.rs` 与相关 runtime/platform 模块一起按 `cfg(feature = "app")` 收口
  - 处理 `newapi_ops.rs` 在 no-app 场景下的 dead code 问题
- 如果走路线 B：
  - 删除/修正文档中关于“关闭 `app` feature 仍完整可编译”的承诺
  - 把验证矩阵写清楚：支持什么、不支持什么

**验收标准**

- 二选一达成：
  - `cargo check --no-default-features` 通过
  - 或文档明确声明该命令不是受支持契约
- `docs/gpui-sigbus-bug.md`、`README.md`、`docs/architecture.md` 与实际行为一致

### F2. 统一 `ProviderManager` 的所有权，消除 reload 后的前后台分叉

**为什么要做**

当前 `ProviderManager` 同时承担“后台刷新运行时注册表”和“前台设置页 token 展示状态解析器”两个角色，但没有单一 owner。`ReloadProviders` 后，后台会切到新 manager，前台仍可能持有旧 manager。

**证据入口**

- `src/runtime/app_state.rs:15-18`
- `src/refresh/coordinator.rs:287-301`
- `src/ui/settings_window/providers/token_input_panel.rs:98-104`

**风险**

- reload 后后台刷新依据的是新 provider 注册表
- 设置页 token 展示状态读取的是旧注册表
- 自定义 provider override 场景最容易出现“前后台看法不一致”

**建议动作**

- 明确单一 owner，建议二选一：
  - 方案 A：`ProviderManager` 由共享状态唯一持有，refresh 只读取共享快照
  - 方案 B：`ReloadProviders` 事件显式把新的 manager/version 同步回 `AppState`
- 如果不愿把 manager 整体放进共享状态，至少要同步一个“provider registry version”并让 UI 重新绑定

**验收标准**

- reload custom providers 后，前后台对 provider metadata / token input capability / override 解析一致
- 补一组测试覆盖 reload 后 UI 读取新 manager 的场景

### F3. 将本地化展示文本从 `ProviderStatus` 中拆出

**为什么要做**

当前 provider 层会在解析阶段直接生成本地化 label、error message、detail text，导致语言切换必须依赖一次刷新才能把缓存中的展示文本“洗掉”。这说明状态层与展示层耦合了。

**证据入口**

- `src/application/reducer.rs:380-387`
- `src/providers/kimi/parser.rs:63-76`
- `src/providers/kimi/parser.rs:95-110`
- `src/providers/error_presenter.rs:10-38`

**建议动作**

- 为 quota / error / detail 建立稳定语义载荷：
  - quota key / display key
  - error reason enum / code
  - detail payload
- 把文案生成下沉到 selector / UI 层
- 过渡期可先只迁移 error text 和 quota label，避免一次性改太大

**验收标准**

- 语言切换不再要求刷新 provider 数据
- 离线状态下切换语言，界面展示仍能统一切换
- `ProviderStatus` 中不再保存主要展示字符串缓存，或至少显著减少

### F4. 为 HTTP client 引入结构化错误边界

**为什么要做**

当前 HTTP 层对错误的表达太粗糙，timeout 之外的大多数错误最终退化成普通字符串，导致 provider 不得不通过 `"status 401"`、`"Unauthorized"` 之类文本匹配来猜测错误类型。

**证据入口**

- `src/providers/common/http_client.rs:43-47`
- `src/providers/common/http_client.rs:64-70`
- `src/providers/mod.rs:145-157`
- `src/providers/gemini/mod.rs:89-103`

**建议动作**

- 在 `http_client` 层返回结构化错误，例如：
  - `Timeout`
  - `Transport`
  - `HttpStatus { code, body }`
- provider 层再把它映射成 `ProviderError`
- 顺手删掉对英文错误字符串的匹配逻辑

**验收标准**

- 401/403/429/5xx 可以稳定分类
- provider 不再依赖 `to_string()` 做认证失败判断
- UI 中 `AuthRequired` / `FetchFailed` / `Unavailable` 分类更稳定

### F5. 做一轮文档对齐专项，修正当前错误入口和过时描述

**为什么要做**

现在不是“个别注释旧了”，而是多份核心文档对 toolchain、feature、theme、tray 实现和测试契约存在冲突，已经会直接误导开发和 review。

**证据入口**

- `README.md:37`
- `docs/architecture.md:5`
- `README.md:150`
- `README.md:155`
- `docs/architecture.md:218`
- `src/tray/README.md:21`
- `src/models/settings/mod.rs:196-201`
- `docs/gpui-sigbus-bug.md:176`
- `docs/gpui-sigbus-bug.md:228`

**建议动作**

- 单独做“文档对齐提交”，不要夹在业务改动里顺手改
- 建议更新顺序：
  - `README.md`
  - `AGENTS.md`
  - `docs/architecture.md`
  - `src/tray/README.md`
  - `docs/gpui-sigbus-bug.md`
  - 相关模块 `README.md`

**验收标准**

- stable / nightly、theme 依赖边界、tray 图标实现、feature 支持矩阵描述一致
- 不再引用已经不存在的路径或旧 hack 作为当前实现

### F6. 清理 `src/providers/antigravity/` 下未参与模块图的旧实现文件

**为什么要做**

这些文件仍位于 `src/` 下，容易被误判为当前有效实现，增加搜索噪音和重构误伤概率。

**证据入口**

- `src/providers/antigravity/cache_source.rs`
- `src/providers/antigravity/live_source.rs`
- `src/providers/antigravity/parse_strategy.rs`
- `src/providers/antigravity/mod.rs`

**建议动作**

- 如果这些文件已无编译入口，直接删除
- 如果必须保留历史追溯，迁出 `src/`，放到 `docs/archive/` 或单独历史目录

**验收标准**

- `src/` 下只保留当前活代码
- 新开发者不会再把这些文件当作可执行实现入口

### F7. 收窄 blanket `allow(...)` 与 dead-code suppress 范围

**为什么要做**

目前不少 dead code / unused import 信号被 blanket suppress 掉，编译器本来可以帮助识别历史残留和 feature 漏洞，现在这些信号被削弱了。

**证据入口**

- `src/platform/mod.rs:1`
- `src/tray/mod.rs:10`
- `src/runtime/ui_hooks.rs:17`
- `src/runtime/ui_hooks.rs:22`
- `src/runtime/ui_hooks.rs:27`
- `src/ui/widgets/card.rs`
- `src/ui/settings_window/mod.rs`

**建议动作**

- 先移除顶层 blanket allow，例如 `src/platform/mod.rs:1`
- 再把确实 unavoidable 的 suppress 收窄到 item 级
- 最后结合 F1 的结论，重新让 dead-code warning 成为有效信号

**验收标准**

- 无全局 blanket allow 掩盖大范围历史问题
- 需要保留的 suppress 都有局部理由，且范围最小化

### F8. 重新定义 placeholder provider 的能力层级和 UI 语义

**为什么要做**

当前 Kilo / OpenCode / Vertex AI 被列在“支持的 provider”中，但其中一些本质上只是 placeholder、提醒或引导入口，不是可监控 provider。现在的抽象把“可发现但不可监控”和“真正可监控”混在一起。

**证据入口**

- `README.md:31-33`
- `docs/providers.md:18-20`

**建议动作**

- 给 provider capability 增加层级，例如：
  - `Monitorable`
  - `Informational`
  - `Placeholder`
- UI 和刷新策略按能力层级分别处理

**验收标准**

- provider 支持表和实际行为一致
- placeholder 不再伪装成“可正常刷新但总是 unavailable”的完整 provider

### F9. 建立 workaround 台账和回收条件

**为什么要做**

当前仓库有一些合理但缺少统一说明的 workaround。问题不是“不能存在 hack”，而是没有标出适用范围、根因和回收条件，后续极易被误删或继续扩散。

**建议先纳入台账的项**

- `src/runtime/settings_window_opener.rs` 中 10ms 延迟打开窗口
- `src/runtime/settings_window_opener.rs` 中 `+1px` resize nudge
- `src/platform/notification.rs` 中每条通知单独线程发送
- `src/refresh/coordinator.rs` 中 timeout guard 只停止等待、不真正取消底层任务

**建议动作**

- 为每个 workaround 补三项信息：
  - 目的
  - 触发条件
  - 未来删除条件
- 可以单独加一份 `docs/architecture.md` 补充节，或建专门的 debt register

**验收标准**

- 主要 workaround 都能追溯“为什么存在”
- 后续重构时能判断哪些可以删、哪些不能动

## 推荐执行顺序

### 第一批，先做契约收口

1. F1 `app` feature 契约
2. F2 `ProviderManager` 所有权
3. F5 文档对齐专项

### 第二批，处理状态和错误边界

1. F3 展示文本下沉
2. F4 结构化 HTTP 错误

### 第三批，做历史清理

1. F6 orphan source files
2. F7 suppress 清理
3. F8 placeholder 语义
4. F9 workaround 台账

## 最后提醒

这轮审查之后，最不应该继续浪费时间追的方向是“默认构建链是不是还坏着”。它已经恢复了。

后续真正值得投入的，是：

- feature 契约是否真实成立
- runtime/provider 的边界是否唯一
- 状态层是否仍混入展示语义
- 文档是否继续输出错误前提
