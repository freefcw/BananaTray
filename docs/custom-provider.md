# 自定义 Provider 使用指南

BananaTray 支持通过 YAML 文件声明自定义 provider，无需编写 Rust 代码。

当前稳定契约是 `schema_version: 2`，运行时只解释 `plan.steps`。旧版顶层 `source` / `parser` YAML 不再作为运行时兼容路径保留；可用 `scripts/migrate_custom_provider_yaml.py` 做一次性迁移。

## 先说结论

- 常见 NewAPI / OneAPI 中转站优先使用设置页里的 NewAPI 表单。
- 需要完全自定义取数逻辑时，优先使用设置页里的“自定义脚本”向导；它会生成脚本文件和 `source.type: cli` 的 YAML。
- 手写 YAML 适合简单 HTTP API、CLI 输出解析、安装检测入口。
- 多端点 API 用 `plan.mode: merge`；多 source fallback 用 `plan.mode: first_success`。
- 当前没有监视 providers 目录的自动 watcher；手工新增或编辑 YAML 后，通常需要重启应用才能重新加载。

## 配置目录

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `$XDG_CONFIG_HOME/bananatray/providers/`

如果 macOS 上存在旧目录 `~/Library/Application Support/bananatray/providers/`，应用启动时会迁移到规范目录。

脚本向导还会把脚本源码保存到同级配置树下的 `scripts/` 目录：

- macOS: `~/Library/Application Support/BananaTray/scripts/`
- Linux: `$XDG_CONFIG_HOME/bananatray/scripts/`

设置页生成或更新的 NewAPI YAML、脚本 YAML 与脚本源码可能包含凭证。BananaTray 会先写入同目录私密临时文件并同步，再替换目标；Unix 上最终文件权限固定为 `0600`。脚本向导保存脚本和 YAML 时保持双文件事务语义，任一替换失败都会恢复旧文件或清理本次新建文件。

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

## 设置页脚本向导

在 `Settings → Providers → Add Provider → Custom Script` 中可以直接创建脚本型 provider。这个入口适合 ccswitch 这类“客户端无法内置所有 API 形态，但用户可以写少量脚本取数”的场景。

保存时 BananaTray 会生成两类文件：

- `providers/script-{slug}.yaml`
  - 标准 `schema_version: 2` 自定义 provider。
  - 使用 `source.type: cli`，命令为表单里的解释器，参数为脚本文件路径。
  - provider id 形如 `{slug}:script`，设置页会把这类 provider 识别为可继续编辑的脚本 provider。若同名或非 ASCII 名称产生相同 slug，向导会自动追加 `-2`、`-3` 等后缀。
- `scripts/script-{slug}.py`
  - 表单中的脚本源码。

脚本向导不新增 runtime schema，也不绕过 provider manager；保存后的刷新仍走 `plan.steps`、availability、parser、fallback 和 hot reload 这套自定义 provider 机制。

删除脚本向导生成的 provider 时，BananaTray 会按 YAML 中记录的实际脚本路径删除 companion script，但只会删除配置树 `scripts/` 目录内的脚本文件；手写 YAML 指向外部脚本时不会删除该外部文件。

### stdout JSON 契约

脚本必须向 stdout 打印一个 JSON 对象。当前稳定字段是：

```json
{
  "ok": true,
  "isValid": true,
  "is_active": true,
  "label": "Balance",
  "remaining": 12.5,
  "used": 3.0,
  "unit": "USD",
  "account_email": "user@example.com",
  "account_tier": "Pro"
}
```

字段说明：

- `remaining`
  - 必填。数值或可解析为数值的字符串。
  - 保存后的 YAML 会把它解析为 `quota_type: credit` 的余额模式。
- `used`
  - 可选。用于展示已用量。
- `unit`
  - 可选。脚本向导生成的 YAML 会把它作为 quota detail 读取；当前 credit quota 主数值仍使用 BananaTray 既有的 `$` 展示规则。
- `label`
  - 可选。`Run Test` 预览使用；当前生成的 YAML 固定展示为 `Balance`。
- `ok` / `isValid` / `is_active`
  - 可选。任一字段显式为 `false` 时，`Run Test` 会视为无效结果。
- `account_email` / `account_tier`
  - 可选。保存后的 YAML 会按这两个字段填充账户信息。

脚本可以从环境变量、文件、HTTP API、CLI 等任意来源取数。建议把密钥放在环境变量或系统凭据中，不要硬编码到脚本里。

### Run Test 与刷新

“Run Test” 会先把当前脚本写入临时目录，用表单中的解释器执行，并按上面的 stdout JSON 契约做预览校验。测试不会保存 provider 文件。

保存后常规刷新走生成的 `source.type: cli`。表单里的 timeout 会写入 YAML 的 `source.timeout_ms`，单位为毫秒；UI 中按秒填写。

脚本测试在后台线程执行，设置窗口会保持可响应；如果用户连续触发测试，只会采纳最新一次测试的回填结果。测试和刷新都会把脚本进程的等待与 stdout/stderr 读取纳入同一个 timeout 窗口。

## 顶层结构

```yaml
schema_version: 2
id: "provider-name:source"
base_url: "https://example.com"   # 可选
metadata: { ... }
plan:
  mode: first_success
  steps:
    - name: "default"
      required: true
      availability: { ... }       # 可选
      source: { ... }
      preprocess: [ ... ]         # 可选
      parser: { ... }             # placeholder source 时可省略
```

字段说明：

- `schema_version`
  - 固定为 `2`。
- `id`
  - 自定义 provider 的稳定标识，必须唯一。
  - NewAPI 表单生成的 provider 使用 `{domain-slug}:newapi`，这类 provider 会在设置页显示“编辑配置”入口。
- `base_url`
  - 可选前缀；step 中的 URL 若以 `/` 开头，会自动拼接该前缀。
- `metadata`
  - 展示名称、品牌、dashboard 链接等。
- `plan`
  - 数据获取与解析计划。运行时只理解这一套执行模型。

## plan

`plan.mode` 支持两种：

- `first_success`
  - 按顺序执行 step，首个成功 step 直接作为刷新结果。
  - 适合 API -> CLI、cookie 候选、主接口失败后的替代来源。
- `merge`
  - 执行多个 step，合并成功 step 的 quotas 和账户信息。
  - `required: false` 的 step 失败不会导致整次刷新失败。
  - 适合“主端点 + 辅助端点”，例如 credits + key limit。

默认 fallback 规则是保守的：timeout、网络错误、5xx、解析失败、无数据等会继续尝试后续 step；认证错误、配置缺失和 429 不会继续盲目 fallback。

provider-level `check_availability(ctx)` 和实际执行阶段都会检查 step availability。原因是预检与执行之间环境可能变化，例如 CLI 被卸载、文件被删除或环境变量被清空。

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

## step availability

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

- `availability` 是 step 级字段。
- `~` 会展开到用户 home 目录。
- 不配置 `availability` 表示该 step 不做前置可用性检查。

## source

当前支持三种 source：

### 1. `http`

```yaml
source:
  type: http
  method: get
  url: "/api/usage"
  timeout_ms: 8000
  auth:
    type: bearer_env
    env_var: "MY_TOKEN"
```

```yaml
source:
  type: http
  method: post
  url: "/api/usage"
  auth:
    type: cookie
    value: "session=...;cf_clearance=..."
  body: '{"scope":"coding"}'
```

说明：

- `method` 支持 `get` / `post`，默认是 `get`。
- `timeout_ms` 可选，不配置时使用全局默认超时。
- `post` 当前发送 JSON body。
- `headers` 可配置额外请求头；加载 YAML 时会按 HTTP 规范校验 header name/value，非法名称或包含 CR/LF 等非法字节的值会让该 provider 拒绝加载。

```yaml
headers:
  - name: "X-Account-Id"
    value: "${MY_ACCOUNT_ID}"
```

### 2. `cli`

```yaml
source:
  type: cli
  command: "mycli"
  timeout_ms: 20000
  args: ["usage", "--json"]
```

说明：

- `timeout_ms` 可选，不配置时使用共享 CLI 默认超时。

### 3. `placeholder`

```yaml
source:
  type: placeholder
  reason: "仅做安装检测，不支持真实额度拉取"
```

说明：

- 所有 step 都是 `placeholder` 时，该 provider 会标记为 `Placeholder` capability。
- 这类 provider 会显示在 UI 中，但不会参与启动 / 周期 / 手动 / Debug 刷新。
- `parser` 可以省略。

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

```yaml
auth:
  type: cookie
  value: "session=...;cf_clearance=..."
```

```yaml
auth:
  type: session_token
  token: "eyJhbGci..."
  cookie_name: "session"
```

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
- URL 字段（如 `source.url`、`login_url`、`dashboard_url`）。以 `/` 开头的 `source.url` / `dashboard_url` 会先和 `base_url` 拼接。
- `headers[].value`
- `login.username`
- `login.password`

如果环境变量不存在，会展开为空字符串，因此更适合内部自用配置，而不是面向非技术用户分发的模板。

## 旧 YAML 迁移

旧版顶层 `availability/source/parser/preprocess` 可以用脚本迁移：

```bash
python3 scripts/migrate_custom_provider_yaml.py ~/Library/Application\ Support/BananaTray/providers --write
```

默认会生成 `.bak` 备份；确认无误后可自行删除。只预览不写入时去掉 `--write`。

迁移规则：

- 顶层 `source` / `parser` 移到 `plan.steps[0]`。
- 顶层 `availability` 移到同一个 step。
- `http_get` / `http_post` 转成 `source.type: http` + `method: get/post`。
- 只转换块式 `source` 映射的直接 `type` 字段；块标量或嵌套内容中的同名文本保持原样。
- 已有 `schema_version` 会原位更新为 `2`，不会再生成重复键。

迁移器采用 fail-closed：开始写入前会预检本次输入的全部文件。发现未知/重复顶层字段、缺少 `id`/`metadata`/`source`/非空 `plan.steps` 等必要结构、非 placeholder legacy source 缺少 `parser`、已有 `plan` 又混入旧字段、不受支持的 `schema_version`、无法唯一识别或可靠转换 `source.type`，或已有 `.bak` 可能被覆盖时，整批退出且不修改任何文件。写入使用同目录临时文件原子替换，并保留原文件权限；默认备份也保留原文件元数据。

## 当前会做的校验

加载阶段当前会明确校验这些问题：

- `schema_version` 必须为 `2`
- `id` 不能为空
- `metadata.display_name` 不能为空
- `plan.steps` 至少包含一个 step
- step `name` 不能为空
- `source.command` / `source.url` 不能为空
- HTTP POST 必须有 `body`
- 自定义 HTTP header name/value 必须符合 HTTP 语法；`header_env.header` 的名称也会在加载时校验
- 非 `placeholder` source 必须配置 `parser`
- `placeholder.reason` 不能为空
- `parser.quotas` 不能为空
- JSON quota 规则必须二选一：`remaining` 或 `used + limit`；`remaining` 不能和 `limit` 同时出现
- 正则表达式和 capture group 必须合法
- `divisor` 必须为正数

有些配置问题不会在加载阶段 fail-fast，而会在实际 refresh 时暴露。这是当前实现边界，不是文档遗漏。

## 故障排查

### Provider 没出现

按顺序检查：

1. YAML 是否位于正确目录
2. 扩展名是否为 `.yaml` 或 `.yml`
3. 是否包含 `schema_version: 2`
4. YAML 语法是否有效
5. 日志里是否有 `providers::custom` 的 warning

### Provider 显示为 Disconnected 或 Unavailable

优先检查：

1. 认证信息是否过期
2. step 的 `availability` 条件是否真的成立
3. `source` 能否独立跑通
4. `parser` 的路径 / 正则是否和实际响应匹配

### 数值不正确

优先检查：

1. JSON 路径或正则是否对应到了正确字段
2. `remaining` / `used + limit` 是否选对模式
3. `divisor` 是否符合站点的真实单位换算

### Cloudflare / CDN 拦截

并非每个站点都有这个问题，但只要脚本访问的站点挂在 Cloudflare、Akamai Bot Manager、AWS WAF 等防护产品后面，原始 `curl` / `requests` / `reqwest` 这类客户端就有可能被识别为机器人。表现通常是：

- HTTP 403 / 503 + 响应体里出现 `Just a moment...` / `Checking your browser` / `Attention Required` / `cf-ray` / `cf_clearance` / `__cf_bm` 等关键字
- 浏览器里手动访问完全正常，但脚本里就是拿不到 JSON
- 脚本向导的 `Run Test` 结果框会自动识别这类响应并给出提示

按代价从低到高的常用做法：

1. **优先走 API key 通道，绕开网页 session 路径。**
   NewAPI / OneAPI 这类站点通常会把 `/api/user/self` 一类网页入口放在 CF 防护后面，但 `/v1/...` 之类需要 `Authorization: Bearer sk-...` 的程序入口往往会在 CF 规则里放行。先翻一下站点文档，能用 API key 就用 API key，这是最干净的方案。
2. **复用浏览器的 `cf_clearance` cookie + 严格匹配 User-Agent。**
   浏览器里登录站点后，从 DevTools 拷贝 `cf_clearance`（必要时还有 `__cf_bm`）和当前 UA。脚本请求里 cookie 串带上这些字段，并且 `User-Agent` header 必须和你拿 cookie 时的浏览器一字不差。`cf_clearance` 与 UA + 出口 IP 强绑定，IP 漂移或 UA 不匹配都会立刻失效；它通常 30 分钟到几小时过期，脚本检测到 403/503 时给一条提示自己重抓即可。
3. **换用带浏览器 TLS 指纹的 HTTP 客户端。**
   普通 `curl` / `requests` / `reqwest` 的 TLS ClientHello 指纹很容易被 CF 直接拒掉。可以换成：
   - Python：[`curl_cffi`](https://github.com/lexiforest/curl_cffi)（API 兼容 requests，`impersonate="chrome124"`）或 `tls-client`
   - Node：`cycletls`
   - 命令行：`curl-impersonate`（独立 binary）

   脚本结构基本不动，只是换个 HTTP 库；这一档对 Bot Fight Mode 一类的轻量拦截通常已经够。
4. **JS Challenge / Turnstile：用一次无头浏览器拿 cookie。**
   如果上面都过不了，只能上 `playwright` / `undetected-chromedriver` / `nodriver` 启动一次浏览器把 `cf_clearance` 拿到，缓存到本地文件，后续请求走方案 2。BananaTray 的脚本是定时被拉起的短命脚本，**不要每次都启动浏览器**，只在 cookie 过期时刷新；否则刷新延迟和资源开销都会很难看。

如果站点同时支持 API key 和网页 session，**优先走 API key**——它不仅绕开 CF，也避免了 cookie 过期导致的间歇性失败。

## 推荐做法

- 先从最接近的示例开始改，而不是从空白 YAML 开始写。
- 简单 API 用一个 step；主端点 + 辅助端点用 `merge`。
- 先让 `source` 跑通，再写 `parser`。
- 对 NewAPI / OneAPI 一类站点，优先使用完整 `cookie` 方式，而不是 `login`。
- 只有当 UI 表单不满足需求时，才手写 NewAPI YAML。
