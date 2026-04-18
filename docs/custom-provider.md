# 自定义 Provider 使用指南

BananaTray 支持通过 YAML 文件声明自定义 provider，无需编写 Rust 代码。

本文件只保留当前稳定可用的工作流和 Schema 摘要。更细的实现细节请以 `docs/examples/` 和当前代码为准。

## 先说结论

- 如果你只是想接入一个常见 NewAPI / OneAPI 中转站，优先使用设置页里的 NewAPI 表单。
- 如果你需要自定义 HTTP / CLI / 安装检测逻辑，再使用手写 YAML。
- 当前没有监视 providers 目录的自动 watcher；手工新增或编辑 YAML 后，通常需要重启应用才能重新加载。

## 配置目录

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `$XDG_CONFIG_HOME/bananatray/providers/`

如果 macOS 上存在旧目录 `~/Library/Application Support/bananatray/providers/`，应用启动时会迁移到规范目录。

## 快速开始

1. 选一个最接近的示例文件。
2. 复制到 providers 目录。
3. 修改站点地址、认证信息和解析规则。
4. 重启 BananaTray。

常用示例：

- `docs/examples/custom-provider-newapi.yaml`
- `docs/examples/custom-provider-http.yaml`
- `docs/examples/custom-provider-cli.yaml`
- `docs/examples/opencode.yaml`
- `docs/examples/kilo.yaml`
- `docs/examples/vertex-ai.yaml`

## 顶层结构

```yaml
id: "provider-name:source"
base_url: "https://example.com"   # 可选
metadata: { ... }
availability: { ... }
source: { ... }
parser: { ... }                   # placeholder source 时可省略
preprocess: [ ... ]               # 可选
```

字段说明：

- `id`
  - 自定义 provider 的稳定标识，必须唯一。
- `base_url`
  - 可选前缀；其他 URL 字段若以 `/` 开头，会自动拼接该前缀。
- `metadata`
  - 展示名称、品牌、dashboard 链接等。
- `availability`
  - 刷新前的可用性检查。
- `source`
  - 真正的数据获取方式。
- `parser`
  - 把原始输出解析成额度数据。
- `preprocess`
  - 在解析前做输出清洗。

## metadata

```yaml
metadata:
  display_name: "My Provider"
  brand_name: "My Brand"
  icon: "M"                  # 可选；留空时会回退到 display_name 首字母
  dashboard_url: "/usage"    # 可选
  account_hint: "account"    # 可选
  source_label: "api"        # 可选
```

## availability

当前支持：

- `always`
- `cli_exists`
- `env_var`
- `file_exists`
- `file_json_match`
- `dir_contains`

示例：

```yaml
availability:
  type: env_var
  value: "MY_API_KEY"
```

```yaml
availability:
  type: file_json_match
  path: "~/.gemini/settings.json"
  json_path: "security.auth.selectedType"
  expected: "vertex-ai"
```

说明：

- `~` 会展开到用户 home 目录。
- 当前 loader 对 `availability` payload 的 fail-fast 校验仍然比较保守；某些空值问题可能要到运行时才暴露。

## source

当前支持四种 source：

### 1. `http_get`

```yaml
source:
  type: http_get
  url: "/api/usage"
  auth:
    type: bearer_env
    env_var: "MY_TOKEN"
```

### 2. `http_post`

```yaml
source:
  type: http_post
  url: "/api/usage"
  auth:
    type: cookie
    value: "session=...;cf_clearance=..."
  body: '{"scope":"coding"}'
```

### 3. `cli`

```yaml
source:
  type: cli
  command: "mycli"
  args: ["usage", "--json"]
```

### 4. `placeholder`

```yaml
source:
  type: placeholder
  reason: "仅做安装检测，不支持真实额度拉取"
```

## auth

HTTP source 当前支持：

- `bearer`
- `bearer_env`
- `header_env`
- `file_token`
- `login`
- `cookie`
- `session_token`

常见场景：

- NewAPI / OneAPI 且需要完整 Cookie：

```yaml
auth:
  type: cookie
  value: "session=...;cf_clearance=..."
```

- 只持有单个 session cookie：

```yaml
auth:
  type: session_token
  token: "eyJhbGci..."
  cookie_name: "session"   # 默认就是 session
```

- 共享环境变量中的 Bearer token：

```yaml
auth:
  type: bearer_env
  env_var: "MY_API_TOKEN"
```

说明：

- `login` 是备选方案，不适合大多数启用了额外登录验证的站点。
- `file_token` 适合复用 CLI 工具写到本地 JSON 文件里的 OAuth token。

## parser

当前支持两种 parser：

### 1. `json`

支持两种额度模式：

- `used + limit`
- `remaining`（余额模式）

```yaml
parser:
  format: json
  account_email: "data.user.email"
  account_tier: "data.plan.name"
  quotas:
    - label: "Monthly"
      used: "data.usage.used"
      limit: "data.usage.limit"
      quota_type: credit
      divisor: 500000
```

```yaml
parser:
  format: json
  quotas:
    - label: "Balance"
      remaining: "data.quota"
      used: "data.used_quota"
      quota_type: credit
      divisor: 500000
```

### 2. `regex`

```yaml
parser:
  format: regex
  account_email: 'Signed in as\\s+(\\S+)'
  quotas:
    - label: "Credits"
      pattern: 'Credits:\\s*(\\d+)/(\\d+)'
      used_group: 1
      limit_group: 2
      quota_type: general
```

## preprocess

当前只支持：

- `strip_ansi`

适用于 CLI 输出带 ANSI 转义、进度条字符或终端噪音的场景。

```yaml
preprocess:
  - strip_ansi
```

## 环境变量展开

以下常见字段当前支持 `${ENV_VAR}` 语法：

- `base_url`
- 各类 URL 字段（如 `source.url`、`login_url`、`dashboard_url`）
- `headers[].value`
- `login.username`
- `login.password`

如果环境变量不存在，会展开为空字符串，因此更适合内部自用配置，而不是面向非技术用户分发的模板。

## 当前会做的校验

加载阶段当前会明确校验这些问题：

- `id` 不能为空
- `metadata.display_name` 不能为空
- `source.command` / `source.url` 不能为空
- `parser.quotas` 不能为空
- 正则表达式和 capture group 必须合法
- `divisor` 必须为正数

有些配置问题不会在加载阶段 fail-fast，而会在实际 refresh 时暴露。这是当前实现边界，不是文档遗漏。

## 故障排查

### Provider 没出现

按顺序检查：

1. YAML 是否位于正确目录
2. 扩展名是否为 `.yaml` 或 `.yml`
3. YAML 语法是否有效
4. 日志里是否有 `providers::custom` 的 warning

### Provider 显示为 Disconnected 或 Unavailable

优先检查：

1. 认证信息是否过期
2. `availability` 条件是否真的成立
3. `source` 能否在命令行里独立跑通
4. `parser` 的路径 / 正则是否和实际响应匹配

### 数值不正确

优先检查：

1. JSON 路径或正则是否对应到了正确字段
2. `remaining` / `used + limit` 是否选对模式
3. `divisor` 是否符合站点的真实单位换算

## 推荐做法

- 先从最接近的示例开始改，而不是从空白 YAML 开始写。
- 先让 `source` 跑通，再写 `parser`。
- 对 NewAPI / OneAPI 一类站点，优先使用完整 `cookie` 方式，而不是 `login`。
- 只有当 UI 表单不满足需求时，才手写 NewAPI YAML。
