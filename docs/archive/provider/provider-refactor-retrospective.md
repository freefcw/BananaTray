# Provider Refactor Retrospective

> 这是重构复盘文档，保留的是当时的判断依据与边界思考。
> 它对“为什么这么设计”仍有价值，但不应被当成当前 provider 目录或实现细节的逐项契约。

这份文档记录本次 `src/providers` 重构最开始的核心审查结论，包括：

- 最初发现的问题是什么
- 为什么这些问题会持续堆积
- 当时是如何判断“该抽什么，不该抽什么”的
- 最终采用了哪些优化方向
- 后续继续演进时应坚持哪些边界

这不是“新增 provider 怎么写”的操作手册。那部分请看 [Provider Blueprints](../../provider-blueprints.md)。

## 背景

`src/providers` 是 BananaTray 最容易自然膨胀的模块之一。原因很简单：

- 不同 provider 的接入协议差异非常大
- 有的走 HTTP API
- 有的走 CLI
- 有的走本地 SQLite / 配置文件 / 缓存
- 有的只能做“安装检测”，不能真正取到配额

这类模块最常见的问题不是“功能写不出来”，而是“每加一个 provider，抽象就更乱一点”。

本次重构的目标不是追求统一外观，而是让 provider 层重新满足两个要求：

1. 新增 provider 时有清晰落点
2. 已有 provider 的差异不会反向污染整体抽象

## 最初发现的核心问题

### 1. 接口信息分散，注册事实与展示事实脱节

重构前，provider 的身份信息分散在多个方法和多个调用点里：

- `kind()`
- `id()`
- `metadata()`

问题不在于“方法多”，而在于这些方法描述的是同一件事的不同切面，却允许分散定义。

直接后果：

- 注册层和 UI 层各自拿自己想要的字段
- provider 身份不再是一个单一事实源
- 新增 provider 时更容易漏改或改不一致

### 2. provider 同时做了取数、解析、编排、错误展示

很多 provider 的单文件里混着几类完全不同的职责：

- 凭证发现
- 请求发送
- 响应解析
- fallback 编排
- 错误提示文案拼接

这会导致两个问题：

- 测试很难只测其中一层
- 一旦想抽共性，就会把错误层级一起抽坏

### 3. “可用性检查”语义不稳定

不同 provider 的 `check_availability()` 语义并不一致：

- 有的表示“主路径可用”
- 有的表示“任一路径可用”
- 有的只是“命令存在”
- 有的则把“暂时刷新失败”也混进不可用

这会让 `ProviderManager` 和 `RefreshCoordinator` 难以稳定推理。

`Antigravity` 是最典型的例子：

- 真实刷新路径是 `live source -> local cache fallback`
- 但旧的 `check_availability()` 只看进程是否存在
- 结果是：本地缓存明明能读，也会被提前判成 unavailable

### 4. 错误分类和错误展示混在 provider 内

provider 层既返回结构化错误，又常常顺手拼接用户提示。

这违反了两个边界：

- provider 应返回事实，不应承担 UI 呈现
- 错误分类应集中，否则同一错误在不同 provider 里会表现不一致

### 5. 重复代码很多，但重复的不是同一个层级

最开始看起来“很多东西都能统一”，但细拆之后发现重复分成三类：

1. 动作重复
2. 编排重复
3. 业务解释重复

只有第一类最适合抽公共模块。

例子：

- CLI 是否存在、命令执行、退出码处理：适合抽
- 多 source fallback 顺序：适合在单个 provider 内显式收敛
- 模型配额如何解释、哪些字段代表计划等级：不适合跨 provider 抽象

### 6. 有过度抽象的风险

审查过程中最危险的点不是“缺抽象”，而是“很容易抽错”。

两个最典型的误判候选：

- 把 `Claude::UsageProbe` 和 `Antigravity::ParseStrategy` 合并成一个统一 trait
- 把占位 provider 也硬拆成多文件目录

这两件事如果做了，代码表面会更统一，但语义会更差。

## 问题为什么会形成

根因不是某一个 provider 写坏了，而是 provider 层天然会受到三种力量拉扯：

- 接入差异非常大
- 新 provider 会持续增加
- UI 和刷新调度层又需要一个统一入口

如果没有刻意维护边界，代码就会自然滑向下面这种状态：

- trait 很薄，但实现很胖
- 公共抽象看起来多，实际含义不稳定
- 编排逻辑和取数细节互相渗透

换句话说，这不是“某几个文件写乱了”，而是“缺少一套判断什么该统一、什么不该统一的准则”。

## 当时的思考过程

本次重构不是从“要设计什么模式”开始的，而是从下面三个问题开始筛：

### 问题 A：哪些内容是跨 provider 的稳定事实

结论：

- provider 身份本身是稳定事实
- 错误分类是稳定事实
- CLI 执行动作是稳定事实

所以最终收敛出了：

- `ProviderDescriptor`
- `ProviderError`
- `ProviderErrorPresenter`
- `providers/common/cli.rs`
- `providers/common/jwt.rs`

### 问题 B：哪些内容只是“看起来像”，其实不在同一层

结论：

- `Claude::UsageProbe` 是 source selection
- `Antigravity::ParseStrategy` 是 payload parsing

二者都带 strategy/fallback 味道，但层级不同：

- 前者回答“从哪里拿数据”
- 后者回答“同一份领域数据如何解码”

所以不应该统一成一个总 trait。

### 问题 C：哪些 provider 值得拆目录，哪些不值得

结论不是“单文件都不好”，而是：

- 多职责且逻辑持续增长的 provider，拆目录
- 只是占位识别或简单桥接的 provider，保持单文件

因此：

- `Codex / Gemini / Cursor / Kimi / MiniMax / Copilot / Claude / Antigravity` 适合多文件
- `Kilo / OpenCode / Vertex AI` 只需要保持简单并可测试

## 最终采用的优化方向

## 方向 1：先收敛“单一事实源”

核心动作：

- 用 `ProviderDescriptor` 收敛 provider ID 和展示元数据
- 让 `AiProvider` 的入口尽量稳定为三件事：
  - `descriptor()`
  - `check_availability()`
  - `refresh()`

意义：

- 注册层、刷新层、UI 层看到的是同一份 provider 身份信息
- provider 不再需要在多个方法里重复声明“我是谁”

## 方向 2：把错误展示从 provider 层剥离

核心动作：

- provider 层只返回 `ProviderError`
- UI message 和 `ErrorKind` 统一交给 `ProviderErrorPresenter`

意义：

- provider 不再关心用户最终看到什么文案
- 错误分类不会因 provider 编写风格不同而漂移

## 方向 3：按稳定职责拆 provider，而不是按技术名词拆

优先顺序是：

1. `auth`
2. `client/source`
3. `parser`
4. `mod.rs` orchestration

不是所有 provider 都要长成同一个目录，但凡拆目录，都应优先沿这个边界判断。

## 方向 4：只把“长期稳定重复动作”提到 `common/`

已经证明稳定的抽象：

- `common/jwt.rs`
- `common/cli.rs`

判断标准：

- 至少 2 个 provider 已重复
- 抽的是动作边界，不是业务理解
- 不会迫使特例 provider 反向适配公共层

## 方向 5：让多 source provider 的编排显式可见

这次最终把两类 source fallback 明确成不同蓝图：

- `Claude`
  - `mod.rs` 负责 source selection
  - `api_probe.rs` / `cli_probe.rs` 负责真实取数
- `Antigravity`
  - `mod.rs` 负责 live/cache fallback
  - `live_source.rs` / `cache_source.rs` 负责 source 边界
  - `parse_strategy.rs` 只处理载荷格式差异

意义：

- fallback 顺序不再藏在大函数里
- `check_availability()` 终于可以与 `refresh()` 的真实路径保持一致

## 本次明确拒绝的“伪统一”

### 1. 不把所有 provider 都拆成目录

原因：

- 占位 provider 逻辑太薄
- 为目录而目录，只会增加文件跳转成本

### 2. 不把 `Claude::UsageProbe` 和 `Antigravity::ParseStrategy` 合并

原因：

- 一个是 source 抽象
- 一个是 parser 抽象
- 共用名字会削弱语义，而不是增强复用

### 3. 不把 provider 变成“万能基类 + 一堆 hook”

原因：

- provider 差异太大
- hook 越多，越会把不稳定差异推到公共层

### 4. 不把用户提示文案继续留在 provider 内

原因：

- provider 应返回结构化事实
- UI 呈现需要单独演进

## 结果概览

本次重构最终形成了下面这几个稳定结果：

- `AiProvider` 入口收敛
- `ProviderDescriptor` 成为 provider 身份单一事实源
- `ProviderErrorPresenter` 统一错误展示
- 多个 provider 完成职责拆分
- `common/jwt.rs`、`common/cli.rs` 提炼完成
- `Claude` 和 `Antigravity` 的 source fallback 边界变清晰
- 占位 provider 保持简单，但可测试

## 对 SOLID / CLEAN 的具体落实

### Single Responsibility Principle

- `mod.rs` 只做 orchestration
- `auth/client/parser/source` 各管一层
- `ProviderErrorPresenter` 负责错误呈现，不放回 provider

### Open/Closed Principle

- 新增 provider 主要通过新增模块和注册完成
- 公共层只吸收稳定动作，不要求改已有 provider 语义

### Liskov / Interface Segregation

- `AiProvider` 保持极小接口
- 不强迫所有 provider 实现无意义的细分方法

### Dependency Inversion

- `ClaudeProvider` 通过 `UsageProbe` 依赖 source 抽象，而不是把 API/CLI 细节写死在编排中

### Clean Architecture

- provider 层产出结构化数据与错误
- 展示层通过 presenter 决定用户看到什么
- source / parser / orchestration 边界清晰，方便分别测试

## 后续继续演进时的准则

### 什么时候该抽公共模块

满足这三个条件再抽：

1. 已经在多个 provider 里真实重复
2. 重复的是动作，不是业务语义
3. 特例 provider 不需要为它委屈自己

### 什么时候该拆目录

满足任一条件就该拆：

- 单文件里同时出现 auth + transport + parse + fallback
- 文件继续增长会显著降低测试粒度
- 后续明显还会扩展

### 什么时候应该拒绝统一

出现以下任一信号就该停手：

- “只是名字很像”
- “抽了以后解释成本更高”
- “公共层开始知道业务细节”
- “特例要为了公共层改写自己的正确实现”

## 推荐阅读顺序

如果后面有人继续改 provider 层，建议按下面顺序阅读：

1. 本文：理解这次重构为什么这么做
2. [Provider Blueprints](../../provider-blueprints.md)：按蓝图选结构
3. [docs/providers.md](../../providers.md)：看当前 provider 全表和接口入口
4. `src/providers/README.md`：看源码侧边界说明

## 一句话结论

这次重构最核心的结论不是“provider 要统一”，而是：

要只统一那些长期稳定、可被证明真的相同的部分；
而把 source 差异、解析差异、占位差异明确地保留在正确层级里。
