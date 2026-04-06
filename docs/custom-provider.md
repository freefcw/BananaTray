# 自定义 Provider 使用指南

BananaTray 支持通过 YAML 文件声明自定义 Provider，无需编写代码即可监控任意 AI 服务的额度用量。

## 快速开始

### 1. 创建配置目录

```bash
# macOS
mkdir -p ~/Library/Application\ Support/bananatray/providers/

# Linux
mkdir -p ~/.config/bananatray/providers/
```

### 2. 创建 YAML 配置文件

将 `.yaml` 或 `.yml` 文件放到上述目录中，应用启动时自动加载。

### 3. 重启 BananaTray

自定义 Provider 会自动出现在主界面中。

---

## 内置模板

项目 `examples/` 目录提供了三种模板，覆盖常见场景：

| 模板 | 文件 | 适用场景 |
|------|------|----------|
| **NewAPI 中转站** | `custom-provider-newapi.yaml` | NewAPI / OneAPI 中转站额度监控 |
| HTTP Provider | `custom-provider-http.yaml` | 通过 HTTP API（POST）获取额度 |
| CLI Provider | `custom-provider-cli.yaml` | 通过命令行工具获取额度 |

---

## NewAPI / OneAPI 中转站配置

这是最常用的场景。NewAPI 是基于 OneAPI 的 AI API 中转站管理系统。

> BananaTray 使用浏览器中已登录的 session cookie 来调用 `/api/user/self` 查询额度。
> 你只需从浏览器中复制 session token 即可，无需输入账号密码。

### 第一步：复制模板

```bash
# macOS
cp examples/custom-provider-newapi.yaml \
   ~/Library/Application\ Support/bananatray/providers/newapi.yaml

# Linux
cp examples/custom-provider-newapi.yaml \
   ~/.config/bananatray/providers/newapi.yaml
```

### 第二步：获取 Session Token

1. 在浏览器中登录你的 NewAPI 站点
2. 打开 DevTools（F12 或 Cmd+Option+I）
3. 切换到 Application → Cookies → 找到你的站点域名
4. 找到名为 `session` 的 cookie，复制其 Value

### 第三步：编辑配置

打开复制后的 `newapi.yaml`，只需修改 2 项：

```yaml
# 你的中转站地址（不含末尾斜杠）
base_url: "https://your-newapi-site.com"

source:
  ...
  auth:
    type: session_token
    token: "your_session_token_here"   # ← 粘贴上一步复制的值
```

所有 API 路径（`/api/user/self`）会自动拼接到 `base_url` 上，无需重复填写。

> ⚠️ session token 有有效期，过期后需重新从浏览器获取并更新配置文件。

### 第四步：调整 divisor（如需）

NewAPI 使用积分制，默认换算关系为 `500000 积分 = $1 USD`。

如果你的站点使用不同的汇率，修改 `divisor` 值：

```yaml
quotas:
  - label: "Balance"
    remaining: "data.quota"     # 剩余额度（余额模式）
    used: "data.used_quota"     # 已用额度（可选，用于展示）
    quota_type: credit
    divisor: 500000    # ← 修改为你站点的实际汇率
```

### 第五步：重启 BananaTray

重启后，系统托盘会显示 NewAPI 的额度余额（如 `$0.50 / $2.00`）。

### 多站点支持

如果你使用多个中转站，创建多个 YAML 文件即可，每个文件使用不同的 `id`：

```yaml
# newapi-site-a.yaml
id: "newapi-a:api"
metadata:
  display_name: "站点 A"
  ...

# newapi-site-b.yaml
id: "newapi-b:api"
metadata:
  display_name: "站点 B"
  ...
```

---

## YAML Schema 完整参考

### 顶层结构

```yaml
id: "provider-name:source"     # 唯一标识符（必填）
base_url: "https://..."        # 基础 URL（可选，其他 URL 字段以 / 开头时自动拼接）
metadata: { ... }              # 展示信息（必填）
availability: { ... }          # 可用性检查（必填）
source: { ... }                # 数据获取方式（必填）
parser: { ... }                # 响应解析规则（必填）
```

**`base_url` 字段：**
设置后，`source.url`、`auth.login_url`、`metadata.dashboard_url` 若以 `/` 开头，会自动拼接 `base_url` 前缀。避免重复填写域名。

### metadata — 展示信息

```yaml
metadata:
  display_name: "My Provider"  # 界面显示名称（必填）
  brand_name: "MyBrand"        # 品牌名称（必填）
  icon: "🤖"                   # 图标 emoji（默认 🤖）
  dashboard_url: "https://..."  # 面板跳转链接（可选）
  account_hint: "account"       # 账户提示文本（默认 "account"）
  source_label: "api"           # 数据源标签（可选）
```

### availability — 可用性检查

在每次刷新前检查目标服务是否可用。

```yaml
# 方式 1：始终可用（推荐，适合认证信息已写在配置中的场景）
availability:
  type: always

# 方式 2：检查 CLI 命令是否存在
availability:
  type: cli_exists
  value: "myai"

# 方式 3：检查环境变量是否设置
availability:
  type: env_var
  value: "MY_API_KEY"

# 方式 4：检查文件是否存在
availability:
  type: file_exists
  value: "~/.myai/config"
```

### source — 数据获取

```yaml
# 方式 1：HTTP GET（最常用）
source:
  type: http_get
  url: "https://api.example.com/usage"
  auth:
    type: bearer
    token: "sk-xxxx"

# 方式 2：HTTP POST
source:
  type: http_post
  url: "https://api.example.com/usage"
  auth:
    type: bearer
    token: "sk-xxxx"
  body: '{"scope":"coding"}'

# 方式 3：CLI 命令
source:
  type: cli
  command: "myai"
  args: ["usage", "--json"]
```

**认证方式：**

| 类型 | 说明 | 适用场景 |
|------|------|----------|
| `session_token` | 用浏览器中的 session token 作为 Cookie 认证 | **NewAPI / OneAPI**（推荐） |
| `cookie` | 直接传递完整的 Cookie 字符串 | 需要传递多个 cookie 的复杂场景 |
| `bearer` | Token 直接写在 YAML 中 | API Key 长期有效的服务 |
| `bearer_env` | 从环境变量读取 Token | 需要与其他工具共享 Token |
| `header_env` | 从环境变量读取自定义 Header | 特殊认证方式 |
| `login` | 先登录获取 session token，再用于请求 | 少数支持自动登录的站点 |

**`session_token` 认证详细配置（推荐）：**

从浏览器中复制 session cookie 即可使用，无需账号密码，兼容所有 NewAPI 站点。

```yaml
auth:
  type: session_token
  token: "eyJhbGci..."              # 从浏览器 Cookie 中复制的 session token
  cookie_name: "session"             # Cookie 名称（默认 "session"，大多数站点无需修改）
```

获取步骤：
1. 在浏览器中登录你的 NewAPI 站点
2. 打开 DevTools（F12 或 Cmd+Option+I）
3. 切换到 Application → Cookies → 找到你的站点域名
4. 找到名为 `session` 的 cookie，复制其 Value
5. 填入 `token` 字段

> ⚠️ session token 有有效期，过期后需重新从浏览器获取并更新配置文件。

**`cookie` 认证详细配置：**

当需要传递多个 cookie 时（如同时需要 `session` 和 `cf_clearance`），可使用完整的 cookie 字符串：

```yaml
auth:
  type: cookie
  value: "session=eyJ...;cf_clearance=abc123"   # 完整的 cookie 字符串
```

**`login` 认证详细配置：**

> ⚠️ 大部分 NewAPI 站点由于启用了 Cloudflare Turnstile 等防登录验证，
> 此方式可能无法使用。推荐优先使用 `session_token` 方式。

```yaml
auth:
  type: login
  login_url: "https://site.com/api/user/login"   # 登录接口 URL
  username: "your_username"                        # 用户名
  password: "your_password"                        # 密码
  token_path: "data"                               # 从响应 JSON 提取 token 的路径（默认 "data"）
```

工作流程：
1. POST `login_url`，body 为 `{"username":"...","password":"..."}`
2. 从响应 JSON 中用 `token_path` 提取 access token
3. 用该 token 作为 `Authorization: Bearer <token>` 进行实际请求

### parser — 响应解析

#### JSON 格式

支持两种模式：

**传统模式**（`used` + `limit`）—— 有进度条：

```yaml
parser:
  format: json
  account_email: "data.user.email"   # 账户邮箱的 JSON 路径（可选）
  account_tier: "data.plan.name"     # 账户等级的 JSON 路径（可选）
  quotas:
    - label: "Monthly"               # 显示标签（必填）
      used: "data.usage.used"        # 已用量的 JSON 路径（必填）
      limit: "data.usage.limit"      # 总额度的 JSON 路径（必填）
      quota_type: credit             # 类型：general / session / weekly / credit
      detail: "data.usage.reset_at"  # 详情文本的 JSON 路径（可选）
      divisor: 500000                # 除数，用于单位换算（可选）
```

**余额模式**（`remaining`）—— 无进度条，仅展示余额和已用：

```yaml
parser:
  format: json
  quotas:
    - label: "Balance"
      remaining: "data.quota"        # 剩余额度的 JSON 路径（必填）
      used: "data.used_quota"        # 已用额度的 JSON 路径（可选，用于展示）
      quota_type: credit
      divisor: 500000
```

> ⚠️ `remaining` 和 `limit` 互斥，不能同时指定。当使用 `remaining` 时，进度条隐藏，
> 卡片以大号数字展示余额，底部展示已用额度。适用于 NewAPI 等只返回剩余额度的场景。

**JSON 路径语法：**
- 点分路径：`data.usage.used` → `json["data"]["usage"]["used"]`
- 数组索引：`items.0.value` → `json["items"][0]["value"]`
- 支持字符串数字自动转换：`"256"` → `256.0`

**divisor 字段：**
提取的数值会自动除以 `divisor`。适用于需要单位换算的场景：
- NewAPI：`500000` 积分 = `$1 USD` → 设置 `divisor: 500000`
- 某些站点 `1000` 积分 = `$1` → 设置 `divisor: 1000`

#### Regex 格式

```yaml
parser:
  format: regex
  account_email: 'Signed in as\s+(\S+)'   # 提取邮箱的正则（可选）
  quotas:
    - label: "Credits"
      pattern: 'Credits:\s*(\d+)/(\d+)'   # 正则表达式（必填）
      used_group: 1                         # used 值的 capture group（默认 1）
      limit_group: 2                        # limit 值的 capture group（默认 2）
      quota_type: general
      divisor: 100                          # 除数（可选）
```

### quota_type — 额度类型

| 类型 | 显示格式 | 适用场景 |
|------|----------|----------|
| `general` | `75 / 100` | 通用计数 |
| `session` | `3 / 5 sessions` | 会话数限制 |
| `weekly` | `75 / 100` | 每周重置的额度 |
| `credit` | `$0.50 / $2.00` | 金额/积分（配合 divisor 换算） |

---

## 环境变量展开（高级）

对于需要从环境变量读取值的高级场景，以下字段支持 `${ENV_VAR}` 语法：

| 字段 | 说明 |
|------|------|
| `source.url` | HTTP 请求 URL |
| `source.headers[].value` | HTTP Header 值 |
| `metadata.dashboard_url` | 面板跳转链接 |

> 大多数用户不需要使用环境变量。直接将值写在 YAML 配置中是更简单的方式。

---

## 故障排查

### Provider 没有出现

1. 检查 YAML 文件是否在正确目录下：
   - macOS: `~/Library/Application Support/bananatray/providers/`
   - Linux: `~/.config/bananatray/providers/`
2. 检查文件扩展名是否为 `.yaml` 或 `.yml`
3. 检查 YAML 语法是否正确（可用 `yq` 或在线 YAML 校验工具）
4. 查看应用日志，搜索 `providers::custom` 相关的 warning

### Provider 显示为 Disconnected

1. 确认 session token 是否已过期，如过期需重新从浏览器获取
2. 用 curl 手动测试：
   ```bash
   curl -H "Cookie: session=your_session_token_here" \
     https://your-site.com/api/user/self
   ```

### 额度数值不正确

1. 检查 JSON 路径是否正确（比对 API 返回的实际 JSON 结构）
2. 对于 NewAPI，确认 `divisor` 值与站点实际汇率匹配

---

## 更多示例

参见项目 `examples/` 目录：
- [custom-provider-newapi.yaml](../examples/custom-provider-newapi.yaml) — NewAPI 中转站（session cookie 认证）
- [custom-provider-http.yaml](../examples/custom-provider-http.yaml) — HTTP POST 模式
- [custom-provider-cli.yaml](../examples/custom-provider-cli.yaml) — CLI 模式
