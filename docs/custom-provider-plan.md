# Custom Provider（YAML 声明式 Provider）实施计划

## 目标

允许用户通过 YAML 文件声明自定义 Provider，无需编写 Rust 代码即可扩展 BananaTray 支持的 AI 工具监控。

## 架构设计

```
~/Library/Application Support/BananaTray/providers/
  my-ai.yaml          ← macOS canonical path
  another-tool.yaml

~/.config/bananatray/providers/
  my-ai.yaml          ← 用户创建的自定义 Provider 定义
  another-tool.yaml
```

```
src/providers/custom/
  mod.rs               ← 模块入口 + CustomProvider（impl AiProvider）
  schema.rs            ← YAML 反序列化结构体
  loader.rs            ← 文件扫描 + 加载 + 校验
  fetcher.rs           ← 数据获取（CLI / HTTP GET / HTTP POST）
  extractor.rs         ← 响应解析（JSON 提取 / 正则提取）
```

### YAML Schema 示例

```yaml
id: "myai:cli"
metadata:
  display_name: "My AI Tool"
  brand_name: "MyCompany"
  icon: "🤖"
  dashboard_url: "https://myai.com/usage"
  account_hint: "MyAI account"
  source_label: "myai cli"

availability:
  type: cli_exists       # cli_exists | env_var | file_exists
  value: "myai"

source:
  type: cli              # cli | http_get | http_post
  command: "myai"
  args: ["usage", "--json"]

parser:
  format: json           # json | regex
  account_email: "$.user.email"
  quotas:
    - label: "Monthly"
      used: "$.usage.used"
      limit: "$.usage.limit"
      type: general
```

### 核心设计决策

1. **ProviderKind 不变** — `ProviderKind` 枚举保持纯编译期。自定义 Provider 在 `ProviderManager` 中独立存储。
2. **ProviderDescriptor.id 改为 `String`** — 从 `&'static str` 改为 `Cow<'static, str>`，兼容动态 ID。
3. **单一 `CustomProvider` 结构体** — 持有解析后的 schema，运行时解释执行，实现 `AiProvider` trait。
4. **惰性集成** — 自定义 Provider 通过 `ProviderManager` 的新方法注册，不影响现有 `ProviderKind::all()` 逻辑。

## 分步实施计划

### Step 1: Schema 定义（`schema.rs`）

定义 YAML 反序列化结构体：
- `CustomProviderDef` — 顶层定义
- `MetadataDef` — 元数据
- `AvailabilityDef` — 可用性检查规则
- `SourceDef` — 数据获取方式
- `ParserDef` — 解析规则
- `QuotaRule` — 单条配额提取规则

### Step 2: Fetcher（`fetcher.rs`）

根据 `SourceDef` 执行数据获取：
- `CliSource` → `Command::new()` + args
- `HttpGetSource` → `http_client::get()`
- `HttpPostSource` → `http_client::post_json()`

认证支持：
- `bearer_env` → 从环境变量读 token，拼 `Authorization: Bearer {token}`

### Step 3: Extractor（`extractor.rs`）

根据 `ParserDef` 解析响应：
- `JsonExtractor` — 用 `.`-分隔的 JSON 路径（如 `data.usage.used`）提取值
- `RegexExtractor` — 用正则 capture groups 提取 used/limit

### Step 4: CustomProvider（`mod.rs`）

实现 `AiProvider` trait：
- `descriptor()` → 从 schema metadata 构建
- `check_availability()` → 根据 availability 规则检查
- `refresh()` → fetcher 获取 → extractor 解析 → 构建 RefreshData

### Step 5: Loader（`loader.rs`）

- 扫描规范配置目录中的 `providers/*.yaml`
- 反序列化 + 校验
- 为每个有效文件创建 `CustomProvider` 实例

### Step 6: ProviderManager 集成

- `ProviderDescriptor.id` 从 `&'static str` 改为 `Cow<'static, str>`
- `ProviderManager` 新增 `register_custom_providers()` 方法
- `ProviderManager::new()` 中调用 loader 加载自定义 Provider

### Step 7: 测试

- schema 反序列化测试
- fetcher CLI/HTTP 单元测试
- extractor JSON/regex 提取测试
- 端到端集成测试（YAML → CustomProvider → RefreshData）
