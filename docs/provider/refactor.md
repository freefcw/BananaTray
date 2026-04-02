## 总结

最终选择第二版方案

## 第一版结论

  src/providers 现在已经有一层统一抽象，但主要还是“按供应商堆实现”。如果目标是更符合 SOLID 和 Clean
  Architecture，关键不是再加一个总 trait，而是把每个 Provider 内部的 4 类职责拆开：发现/认证、采集、解析、编
  排。

  现状共性

  - 所有 Provider 都遵循同一条隐式流水线：is_available -> 获取凭证/环境 -> 调用 API/CLI -> 解析 quota -> 组装
    RefreshData。入口在 src/providers/mod.rs:238。
  - 都使用统一错误类型 ProviderError，这点是好的，见 src/providers/mod.rs:31。
  - 都由 ProviderManager 统一注册和调度，见 src/providers/manager.rs:22。
  - 大部分实现都依赖相同基础设施：文件系统、环境变量、CLI、HTTP、时间格式化。

  现状特性

  - 单源 API 型：Gemini、Kimi、MiniMax、Copilot、Codex
  - 单源 CLI 型：Amp、Kiro
  - 混合源/回退型：Claude、Cursor、Antigravity
  - 占位型：Kilo、OpenCode、Vertex AI

  当前设计里最值得保留的部分

  - ProviderError 的结构化建模是对的。
  - Claude 的 UsageProbe 已经开始体现“源选择/策略”思想，见 src/providers/claude/probe.rs:21。
  - Antigravity 的 ParseStrategy 已经开始体现“解析器可替换”，见 src/providers/antigravity/
    parse_strategy.rs:6。

  主要问题

  - AiProvider 同时暴露 refresh 和 refresh_quotas，职责不清，违反 ISP。见 src/providers/mod.rs:257。
  - Provider 文件里混合了领域逻辑、基础设施、错误展示、重试和 token 持久化，SRP 不够好。Codex 是典型例子，见
    src/providers/codex.rs:23。
  - ProviderError 自带 format_for_display()，把领域错误和 UI/i18n 绑在一起，违反 Clean 的依赖方向。见 src/
    providers/mod.rs:203。
  - ProviderManager::refresh_provider() 在 manager 层直接拼接英文错误字符串，而不是返回领域错误，边界有污染。
    见 src/providers/manager.rs:86。
  - 重复代码明显。Amp 同一次 CLI 解析写了两遍，见 src/providers/amp.rs:44 和 src/providers/amp.rs:89。
  - Copilot 的 token 解析、缓存、API 调用、DTO 解析都在一个模块里，模块边界太粗。见 src/providers/copilot/
    mod.rs:79。
  - 大量 anyhow::Result 让接口契约变弱，应用层只能二次 classify，见 src/refresh.rs:167。

  符合 SOLID/CLEAN 的推荐设计

  分三层就够，不要过度抽象。

  1. domain

  - 只放稳定概念：ProviderSnapshot、AccountInfo、ProviderDescriptor、ProviderFailure
  - 不依赖 http、Command、serde_json::Value、i18n

  2. application

  - 只放用例：RefreshProviderUseCase
  - 负责调度 provider、做 fallback policy、错误到 UI 的映射
  - ProviderFailurePresenter 放这里，不放 provider 层

  3. infrastructure/providers

  - 每个 Provider 只负责接外部世界
  - 内部分成 auth.rs、source/*.rs、parser.rs、mod.rs

  建议的核心接口

  pub struct ProviderSnapshot {
      pub quotas: Vec<QuotaInfo>,
      pub account: Option<AccountInfo>,
  }

  pub struct AccountInfo {
      pub email: Option<String>,
      pub plan: Option<String>,
  }

  pub enum ProviderFailure {
      Unavailable { reason: String },
      AuthRequired,
      SessionExpired,
      ConfigMissing { key: String },
      ParseFailed { reason: String },
      NetworkFailed { reason: String },
      NoData,
      Unsupported,
  }

  #[async_trait::async_trait]
  pub trait QuotaProvider: Send + Sync {
      fn descriptor(&self) -> &'static ProviderDescriptor;
      async fn check(&self) -> Result<(), ProviderFailure>;
      async fn fetch(&self) -> Result<ProviderSnapshot, ProviderFailure>;
  }

  关键原则

  - SRP：Provider facade 只做编排，不做解析细节。
  - OCP：新增 Provider 时主要新增 auth/source/parser 组合，不改中心逻辑。
  - LSP：所有 Provider 都返回同一个 ProviderSnapshot，不要有的返回 quota、有的返回半成品。
  - ISP：把“可用性检查”和“抓取快照”作为明确接口，不保留 refresh/refresh_quotas 双入口。
  - DIP：RefreshCoordinator 依赖 QuotaProvider 抽象，不依赖具体 Command/http/json。

  我建议的目录

  src/providers/
    mod.rs                  // 只保留 registry + trait re-export
    registry.rs
    common/
      cli.rs
      http.rs
      jwt.rs
      sqlite.rs
      oauth.rs
    claude/
      mod.rs                // facade/orchestrator
      auth.rs
      source_api.rs
      source_cli.rs
      parser.rs
      fallback.rs
    codex/
      mod.rs
      auth.rs
      source.rs
      parser.rs
    cursor/
      mod.rs
      auth.rs
      source.rs
      parser.rs

  一个可落地的 Provider 模板

  pub struct CodexProvider {
      auth: CodexAuthStore,
      source: CodexUsageSource,
      parser: CodexUsageParser,
  }

  #[async_trait::async_trait]
  impl QuotaProvider for CodexProvider {
      fn descriptor(&self) -> &'static ProviderDescriptor {
          &CODEX_DESCRIPTOR
      }

      async fn check(&self) -> Result<(), ProviderFailure> {
          self.auth.check()
      }

      async fn fetch(&self) -> Result<ProviderSnapshot, ProviderFailure> {
          let session = self.auth.load_session()?;
          let raw = self.source.fetch(&session).await?;
          self.parser.parse(&raw)
      }
  }

  针对当前代码的具体拆分建议

  - Amp
      - 提取 run_usage() 和 parse_usage_output()
      - refresh() 与 refresh_quotas() 共用一次解析结果
  - Gemini
      - credentials_path/load_credentials/check_auth_type 放 auth.rs
      - fetch_quota_via_api 放 source.rs
      - simplify_model_name + response->QuotaInfo 放 parser.rs
  - Codex
      - token 刷新与 auth.json 写回独立成 auth.rs
      - API headers/body 双解析独立成 parser.rs
  - Cursor
      - sqlite3 读取 token 不应混在 provider facade；做成 CursorTokenStore
  - Copilot
      - resolve_token 与缓存机制独立成 token_resolver.rs
      - API DTO 和 quota mapping 独立成 parser.rs
  - Claude
      - 现有 UsageProbe 可升级为通用 Source 抽象，已经最接近目标设计
  - Antigravity
      - detect_process/discover_port/post_api/read_local_cache 是 source 层
      - ParseStrategy 保留，已经是对的

  错误处理建议

  把这个：

  - ProviderError::format_for_display()
  - RefreshCoordinator::classify_error_kind()

  改成应用层单独的 presenter/mapper。

  pub trait ProviderFailurePresenter {
      fn message(&self, failure: &ProviderFailure) -> String;
      fn kind(&self, failure: &ProviderFailure) -> ErrorKind;
  }

  这样 provider 只产出“事实”，UI 再决定怎么说。

  迁移顺序

  1. 先改接口，只保留 fetch() -> Result<ProviderSnapshot, ProviderFailure>
  2. 先重构 Amp、MiniMax、Kimi 这类简单 Provider，建立模板
  3. 再迁移 Codex、Gemini、Cursor
  4. 最后迁移 Claude、Antigravity 这类多源/回退 Provider

## 第二版，重新思考后的输出

• 重新判断

  上一个方案方向对，但还不够清晰，也不够“贴着现有代码走”。

  主要问题有 3 个：

  1. 过于架构化
      - 我把 domain / application / infrastructure 讲得太完整，概念上对，但对这个仓库来说偏“大改造”。
      - src/providers 本身就是基础设施层，没必要为了“看起来更 Clean”再强行加很多层。
  2. 抽象粒度偏大
      - 上个方案里引入了新的 ProviderSnapshot、更重的用例层。
      - 实际上你项目里已经有 src/models/quota.rs 这种结果模型，完全可以复用，没必要再造一个平行概念。
  3. 没把“该保留什么、不该抽象什么”说透
      - 不是所有 Provider 都需要 trait 化。
      - Rust 里过度泛型 + 过多 trait，最后常常比现在更难维护。

  所以，更好的方法不是“大重构成标准 Clean Architecture”，而是：

  保留外部接口，重构内部结构。

  ———

  更好的方案

  核心原则只有 4 条：

  1. AiProvider 保留为应用层唯一入口
  2. RefreshData 保留为统一输出模型
  3. 只在“真正有多实现”的地方用 trait/strategy
  4. Provider 内部拆职责，不拆成过多架构层

  ———

  比之前更合适的目标结构

  先不推翻 src/providers/mod.rs:238，但把它收敛成单一职责接口：

  #[async_trait]
  pub trait AiProvider: Send + Sync {
      fn descriptor(&self) -> &'static ProviderDescriptor;
      async fn check_availability(&self) -> Result<(), ProviderError>;
      async fn refresh(&self) -> Result<RefreshData, ProviderError>;
  }

  这里有几个关键点：

  - 用 descriptor() 取代现在的 metadata() + id() + kind()
  - 去掉 refresh_quotas()，统一只保留 refresh()
  - check_availability() 返回 Result<(), ProviderError>，不要再用 bool

  这比当前更清楚，因为当前接口有几个问题：

  - metadata() / id() / kind() 信息分散，重复
  - is_available() -> bool 丢失原因
  - refresh() / refresh_quotas() 双入口让职责变模糊

  对应当前代码：

  - src/providers/mod.rs:240
  - src/providers/manager.rs:86

  ———

  更好的抽象边界

  不是把所有 Provider 抽成一堆 trait，而是把它们分成 4 种“模式”。

  1. ApiOnly

  - Gemini
  - Kimi
  - MiniMax
  - Copilot
  - Codex

  统一流水线：

  auth -> request -> parse -> map RefreshData

  2. CliOnly

  - Amp
  - Kiro

  统一流水线：

  detect cli -> run cli -> parse stdout/stderr -> map RefreshData

  3. MultiSourceFallback

  - Claude
  - Antigravity
  - Cursor

  统一流水线：

  source A -> source B fallback -> parse -> map RefreshData

  4. UnsupportedPlaceholder

  - Kilo
  - OpenCode
  - VertexAi

  统一流水线：

  detect maybe installed -> return Unsupported/Unavailable

  这比上一个答案更实用，因为它是按你现有代码真实差异分的，不是按书本分层分的。

  ———

  真正应该抽象的地方

  只有两类地方值得用 trait。

  1. 多数据源切换

  - Claude 的 API/CLI
  - Antigravity 的 API/本地缓存

  这里你现在已经有雏形了：

  - src/providers/claude/probe.rs:21
  - src/providers/antigravity/parse_strategy.rs:6

  2. 错误展示

  - ProviderError 不应该自己负责 i18n 展示
  - format_for_display() 应该挪到上层 presenter/mapper

  当前问题在这里：

  - src/providers/mod.rs:203
  - src/refresh.rs:167

  更合理的是：

  pub struct ProviderErrorPresenter;

  impl ProviderErrorPresenter {
      pub fn to_message(error: &ProviderError) -> String { ... }
      pub fn to_error_kind(error: &ProviderError) -> ErrorKind { ... }
  }

  这样 provider 只返回“事实”，UI/refresh 再决定怎么显示。

  ———

  不应该抽象的地方

  这是我上次没强调够的。

  以下内容不建议全 trait 化：

  - Gemini 的 token 文件读取
  - Codex 的 auth.json 刷新逻辑
  - Cursor 的 sqlite token 读取
  - Copilot 的 token cache

  这些更适合拆成模块，而不是 trait。

  例如 Codex 更适合：

  src/providers/codex/
    mod.rs       // facade / orchestrator
    auth.rs      // auth.json 读写 + refresh token
    client.rs    // API call
    parser.rs    // response -> RefreshData

  而不是：

  - TokenStore trait
  - AuthRefresher trait
  - UsageFetcher trait
  - UsageParser trait

  后者太重。

  ———

  比之前更简洁的目录建议

  src/providers/
    mod.rs
    manager.rs
    error.rs
    descriptor.rs
    common/
      cli.rs
      http.rs
      jwt.rs
      json.rs
    amp/
      mod.rs
      parser.rs
    codex/
      mod.rs
      auth.rs
      client.rs
      parser.rs
    gemini/
      mod.rs
      auth.rs
      client.rs
      parser.rs
    claude/
      mod.rs
      source_api.rs
      source_cli.rs
      parser.rs

  注意：

  - 不是所有 Provider 都必须拆 4 个文件
  - 只有复杂 Provider 才拆
  - 简单 Provider 保持 2~3 文件即可

  ———

  对当前代码最重要的 5 个改动

  如果只做最值钱的改动，我会按这个顺序来：

  1. 合并 Provider 元信息

  - 引入 ProviderDescriptor
  - 合并 id/kind/metadata
  - 解决重复定义问题

  2. 删除 refresh_quotas()

  - 统一只保留 refresh() -> RefreshData
  - quota-only 的 provider 直接 RefreshData::quotas_only(...)

  3. 把 ProviderError::format_for_display() 移出 provider 层

  - Provider 不负责展示
  - refresh/app 层负责呈现

  4. is_available() -> bool 改为 check_availability() -> Result<(), ProviderError>

  - 避免 manager 自己拼字符串错误
  - 现在 src/providers/manager.rs:86 这层知道得太少

  5. 按 Provider 类型抽模板重构

  - 先做 Amp
  - 再做 Codex
  - 再复制模式到其他实现

  ———

  为什么这个方法比上一个更好

  因为它同时满足 3 件事：

  1. 更符合 SOLID

  - SRP：provider facade 只做编排
  - OCP：新增 source/parser 不改 manager
  - DIP：上层依赖 AiProvider，不是具体 provider

  2. 更符合你当前项目规模

  - 不需要一口气做完整 Clean Architecture 改造
  - 不会把 Rust 代码变成 trait/generic 迷宫

  3. 更容易落地

  - 可以渐进迁移
  - 不影响现有 refresh.rs、app_state 的整体工作流

  ———

  最终修正后的判断

  上一个方案“理论更完整”，但这次这个方案“工程上更正确”。

  更好的方法不是：

  - 全面重做 providers 架构

  而是：

  - 保留 AiProvider 作为外部稳定接口
  - 统一输出 RefreshData
  - 把 descriptor、error presenter、source/parser 三个边界理顺
  - 只在多源/多解析场景使用 trait
