# Custom Provider（YAML 声明式 Provider）

允许用户通过 YAML 文件声明自定义 Provider，无需编写 Rust 代码。

## 使用方法

将 YAML 文件放到规范配置目录中，应用启动时自动加载：

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `~/.config/bananatray/providers/`
- macOS 如存在旧目录 `~/Library/Application Support/bananatray/providers/`，应用会在启动时自动迁移到规范目录

示例文件见 `docs/examples/` 目录。

详细使用指南见 [docs/custom-provider.md](../../../docs/custom-provider.md)。

## 模块结构

```
custom/
  mod.rs          — 模块入口，re-export
  schema.rs       — YAML v2 plan/step 反序列化结构体
  extractor.rs    — 响应解析（JSON 路径提取 / 正则匹配）
  plan.rs         — 编译后的执行计划，集中处理 step availability / fallback / merge
  provider.rs     — CustomProvider 门面（impl AiProvider，转发 descriptor / availability / refresh）
  descriptor.rs   — ProviderDescriptor 与默认首字母 icon 生成
  locator.rs      — 按 YAML `id` 定位 provider 文件的内部 helper（供 api / generator 复用）
  availability.rs — availability 规则解释执行（CLI / env / file / JSON / dir）
  auth.rs         — auth/header 解析、环境变量凭证、file token、login token
  fetch.rs        — source 解释执行（CLI / HTTP / placeholder）与 preprocess
  url.rs          — base_url 拼接、${ENV_VAR} 展开、~ 路径展开
  log_utils.rs    — 日志截断与认证 header 脱敏
  json_file.rs    — 本地 JSON 文件读取公共基础设施
  loader.rs       — 文件扫描 + 加载 + 校验
  generator.rs    — NewAPI / Script Provider YAML 生成 + 纯解析辅助（由 api 持有外部契约）
  api.rs          — runtime / settings window 唯一入口：filename、默认模板、加载、保存、删除
```

## 设计原则

- **SRP**: 每个模块职责单一（schema 定义 / plan 编排 / 可用性检查 / 认证 / 获取 / 解析 / Provider 门面 / 文件加载）
- **OCP**: 新增自定义 Provider 只需添加 YAML 文件，不修改任何 Rust 代码
- **DIP**: CustomProvider 依赖 descriptor / plan 的窄接口，不直接依赖 availability / fetch / extractor 的执行细节
- **Identity over filename**: 自定义 provider 的真实身份来自 YAML `id`，不是文件名；编辑/删除等流程必须通过 `locator.rs` 按 `id` 定位真实文件
- **Single façade**: runtime / UI 只允许依赖 `api.rs`，不要直接碰 `generator.rs` / `locator.rs` / `schema.rs`
- **Enforced information hiding**: `generator.rs` / `locator.rs` / `schema.rs` 的模块可见性限制在 `providers::custom` 内部，不只是调用约定

## 支持的数据获取方式

| type       | 说明 |
|------------|------|
| `cli`        | 执行 CLI 命令，获取 stdout/stderr，支持 `timeout_ms` |
| `http`       | HTTP GET / POST 请求，支持 `method` 和 `timeout_ms` |
| `placeholder`| 占位：不获取数据，仅检测安装状态；运行时 capability 为 `Placeholder` |

`placeholder` source 的稳定语义：

- 会显示在 provider 列表里，方便保留入口或安装检测。
- 不参与正常刷新，也不会在 UI 中显示 retry / refresh 动作。
- 可省略 `parser`；即使配置了也不会把它变成 monitorable provider。

## Plan 语义

YAML 顶层固定使用 `schema_version: 2` 和 `plan.steps`。

| mode | 说明 |
|------|------|
| `first_success` | 按顺序执行 step，首个成功结果作为刷新结果 |
| `merge` | 执行多个 step，合并成功 step 的 quotas；`required: false` 的失败不阻断刷新 |

旧版顶层 `availability/source/parser/preprocess` 不再是运行时兼容路径；一次性迁移脚本为 `scripts/migrate_custom_provider_yaml.py`。

## 支持的认证方式

| auth type       | 说明 |
|-----------------|------|
| `cookie`        | 直接传递完整的 Cookie 字符串（NewAPI/OneAPI 推荐） |
| `session_token` | 用单个 session cookie 值认证（无 CDN 防护的简单站点） |
| `bearer`        | Token 直接写在 YAML 配置中 |
| `bearer_env`    | 从环境变量读取 token，设置 `Authorization: Bearer {token}` |
| `header_env`    | 从环境变量读取值，设置自定义 header |
| `file_token`    | 从本地 JSON 文件读取 token（CLI 工具 OAuth 凭据） |
| `login`         | 先登录获取 token 再用于请求（备选，部分站点可能不支持） |

## 支持的可用性检查

| type          | 说明 |
|---------------|------|
| `always`         | 始终可用（推荐，适合认证信息已在配置中的场景） |
| `cli_exists`     | 检查 CLI 命令是否存在 |
| `env_var`        | 检查环境变量是否设置 |
| `file_exists`    | 检查文件是否存在（支持 ~ 展开） |
| `file_json_match`| 检查 JSON 文件内容是否匹配指定路径 + 值 |
| `dir_contains`   | 检查目录中是否包含匹配前缀的条目 |

## 支持的解析方式

| format  | 说明 |
|---------|------|
| `json`  | 点分路径提取（如 `data.usage.used`），支持数组索引（如 `items.0.value`） |
| `regex` | 正则 capture group 提取 used/limit 值 |

## 环境变量展开

以下字段支持 `${ENV_VAR}` 语法，在运行时自动用环境变量值替换：

| 字段 | 说明 |
|------|------|
| `base_url` | 基础 URL，可配合相对路径使用 |
| `plan.steps[].source.url` | HTTP 请求 URL（如 `${NEWAPI_BASE_URL}/api/user/self`） |
| `plan.steps[].source.headers[].value` | HTTP header 值 |
| `plan.steps[].source.auth.login_url` | login auth 的登录 URL，可为相对路径 |
| `plan.steps[].source.auth.username` | login auth 用户名 |
| `plan.steps[].source.auth.password` | login auth 密码 |
| `metadata.dashboard_url` | 面板跳转链接 |

如果 URL 字段以 `/` 开头，会先和 `base_url` 拼接；环境变量不存在时会展开为空字符串并写 warning 日志。

## 数值变换

配额提取规则支持 `divisor` 可选字段，提取的 `used` 和 `limit` 数值会自动除以此值。
适用于需要单位换算的场景（如 NewAPI 积分 → 美元）：

```yaml
quotas:
  - label: "Balance"
    used: "data.used_quota"
    limit: "data.quota"
    quota_type: credit
    divisor: 500000  # 500000 积分 = $1 USD
```
