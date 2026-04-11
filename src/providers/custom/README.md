# Custom Provider（YAML 声明式 Provider）

允许用户通过 YAML 文件声明自定义 Provider，无需编写 Rust 代码。

## 使用方法

将 YAML 文件放到规范配置目录中，应用启动时自动加载：

- macOS: `~/Library/Application Support/BananaTray/providers/`
- Linux: `~/.config/bananatray/providers/`
- macOS 如存在旧目录 `~/Library/Application Support/bananatray/providers/`，应用会在启动时自动迁移到规范目录

示例文件见 `docs/examples/` 目录。

详细使用指南见 [docs/custom-provider.md](../../docs/custom-provider.md)。

## 模块结构

```
custom/
  mod.rs          — 模块入口，re-export
  schema.rs       — YAML 反序列化结构体
  extractor.rs    — 响应解析（JSON 路径提取 / 正则匹配）
  provider.rs     — CustomProvider（impl AiProvider）
  loader.rs       — 文件扫描 + 加载 + 校验
  generator.rs    — NewAPI 中转站 YAML 生成 + 配置回读
```

## 设计原则

- **SRP**: 每个模块职责单一（schema 定义 / 获取 / 解析 / Provider 实现 / 文件加载）
- **OCP**: 新增自定义 Provider 只需添加 YAML 文件，不修改任何 Rust 代码
- **DIP**: CustomProvider 依赖 fetcher/extractor 的函数接口，不依赖具体实现细节

## 支持的数据获取方式

| type       | 说明 |
|------------|------|
| `cli`        | 执行 CLI 命令，获取 stdout/stderr |
| `http_get`   | HTTP GET 请求 |
| `http_post`  | HTTP POST 请求（JSON body） |
| `placeholder`| 占位：不获取数据，仅检测安装状态 |

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
| `source.url` | HTTP 请求 URL（如 `${NEWAPI_BASE_URL}/api/user/self`） |
| `source.headers[].value` | HTTP header 值 |
| `metadata.dashboard_url` | 面板跳转链接 |

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
