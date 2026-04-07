# Codeium-family Providers — Antigravity / Windsurf 架构与运行时校验

## 概述

BananaTray 现在将 **Antigravity** 和 **Windsurf** 视为两个独立的 built-in provider：

- `antigravity:api`
- `windsurf:api`

它们在 UI、注册表、ProviderKind、图标、显示名、错误提示上都保持独立身份；
但在实现层共享同一套 **Codeium-family** 底层能力：

- 本地 language server 进程发现
- 本地 API 请求（gRPC-over-HTTP / Connect 风格）
- 本地 SQLite cache fallback
- JSON / protobuf 解析

共享代码位于：

- [src/providers/codeium_family/mod.rs](../src/providers/codeium_family/mod.rs)
- [src/providers/codeium_family/spec.rs](../src/providers/codeium_family/spec.rs)
- [src/providers/codeium_family/live_source.rs](../src/providers/codeium_family/live_source.rs)
- [src/providers/codeium_family/cache_source.rs](../src/providers/codeium_family/cache_source.rs)
- [src/providers/codeium_family/parse_strategy.rs](../src/providers/codeium_family/parse_strategy.rs)

Provider facade 位于：

- [src/providers/antigravity/mod.rs](../src/providers/antigravity/mod.rs)
- [src/providers/windsurf.rs](../src/providers/windsurf.rs)

## 设计原则

### 1. provider 身份独立

不要把 Windsurf 折叠进 Antigravity provider。

原因：

- 用户在 UI 上需要看到独立 provider
- `ProviderKind` / metadata / icon / settings 都按 provider 身份组织
- 运行时进程识别规则不同
- 本地 cache 路径与 key 候选可能不同

因此 BananaTray 采用：

- **独立 provider 身份**
- **共享底层实现**

### 2. 只参数化稳定差异

`CodeiumFamilySpec` 只承载稳定且长期存在的产品差异：

- provider id
- `ProviderKind`
- display / brand / icon metadata
- `ide_name`
- cache DB 路径
- auth-status key candidates
- process markers
- unavailable message / log label

共享层负责通用流程；provider facade 只提供 spec。

## 数据获取流程

```text
1. pgrep 检测 language_server_macos / language_server_macos_arm 进程
2. 用 process markers 判断该进程属于 Antigravity 还是 Windsurf
3. 从进程参数提取：
   - --csrf_token
   - --extension_server_port
4. 用 lsof 查找 LISTEN 端口
5. 依次尝试候选 endpoint：
   - https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus
   - http://127.0.0.1:{extension_port}/exa.language_server_pb.LanguageServerService/GetUserStatus
   - http://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus
6. Body 使用 provider-specific ideName：
   - antigravity => {"metadata":{"ideName":"antigravity"}}
   - windsurf    => {"metadata":{"ideName":"windsurf"}}
7. 如果 live source 失败，则回退本地 state.vscdb cache
```

## 共享实现分层

### `spec.rs`

定义 `CodeiumFamilySpec`，并内置：

- `ANTIGRAVITY_SPEC`
- `WINDSURF_SPEC`

其中当前关键差异如下：

| 字段 | Antigravity | Windsurf |
|---|---|---|
| provider id | `antigravity:api` | `windsurf:api` |
| ide name | `antigravity` | `windsurf` |
| cache DB | `~/Library/Application Support/Antigravity/User/globalStorage/state.vscdb` | `~/Library/Application Support/Windsurf/User/globalStorage/state.vscdb` |
| auth key candidates | `antigravityAuthStatus` | `windsurfAuthStatus`, fallback `antigravityAuthStatus` |
| process markers | `--app_data_dir antigravity`, `/antigravity/`, `.antigravity/`, `/antigravity.app/` | `--ide_name windsurf`, `/windsurf/`, `.windsurf/`, `/windsurf.app/` |

### `live_source.rs`

负责：

- 发现本地 language server
- 识别 provider 所属进程
- 提取 csrf token / port
- 探测 endpoint
- 调用 `GetUserStatus`

### `cache_source.rs`

负责：

- 定位 `state.vscdb`
- 读取 `ItemTable`
- 按 candidate key 查找 auth status
- 解 base64 protobuf payload

### `parse_strategy.rs`

负责解析两种载荷：

- API JSON
- cache protobuf

并统一输出：

- model quotas
- email
- plan name

## GetUserStatus 关键结构

```json
{
  "userStatus": {
    "email": "user@example.com",
    "userTier": {
      "id": "g1-ultra-tier",
      "name": "Google AI Ultra"
    },
    "planStatus": {
      "planInfo": {
        "planName": "Pro"
      }
    },
    "cascadeModelConfigData": {
      "clientModelConfigs": [
        {
          "label": "Claude Opus 4.6 (Thinking)",
          "quotaInfo": {
            "remainingFraction": 1.0,
            "resetTime": "2026-04-01T07:42:12Z"
          }
        }
      ]
    }
  }
}
```

## 订阅等级识别注意点

### 问题

`planStatus.planInfo.planName` 对更高等级用户可能不够可靠，曾出现高等级用户依然显示为 `"Pro"` 的情况。

### 当前策略

解析时优先读取：

- `userTier.name`

回退到：

- `planStatus.planInfo.planName`

即：

```rust
let plan_name = user_status
    .pointer("/userTier/name")
    .and_then(|v| v.as_str())
    .filter(|s| !s.is_empty())
    .or_else(|| {
        user_status
            .pointer("/planStatus/planInfo/planName")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    })
    .map(|s| s.to_string());
```

## 配额提取

每个模型的 quota 来自：

- `cascadeModelConfigData.clientModelConfigs[].label`
- `cascadeModelConfigData.clientModelConfigs[].quotaInfo.remainingFraction`
- `cascadeModelConfigData.clientModelConfigs[].quotaInfo.resetTime`

转换公式：

```text
used_percent = (1.0 - remainingFraction) * 100.0
```

## 回退策略

Codeium-family provider 的刷新策略是：

1. 优先 live source
2. live source 失败时回退 local cache
3. 两者都失败时返回结构化 `FetchFailed`

这层回退由 [src/providers/codeium_family/mod.rs](../src/providers/codeium_family/mod.rs) 统一 orchestrate。

## 运行时校验建议

虽然当前实现已经通过测试，但不同机器上的真实安装状态仍建议做一次本地校验，尤其是 Windsurf：

- cache DB 路径是否存在
- auth-status key 实际名称
- process args 是否包含预期 marker
- `--csrf_token` / `--extension_server_port` 是否可提取
- `lsof` 是否能发现 LISTEN 端口

### 内置调试 helper

可直接运行：

```bash
cargo run -- debug-codeium-family all
cargo run -- debug-codeium-family antigravity
cargo run -- debug-codeium-family windsurf
```

这个命令会输出：

- provider id / ide_name
- candidate cache DB 路径及是否存在
- 本地 cache 中的关键 key 候选是否存在
- 发现到的相关 ItemTable keys
- 匹配到的 language server 进程行
- 提取出的 pid / masked csrf token / extension port
- `lsof` 发现的 listen port
- 推导出的 endpoint hints

## 已知限制

1. Windsurf 的真实本地 key 名在不同版本中可能变化
   - 当前实现优先 `windsurfAuthStatus`
   - 回退 `antigravityAuthStatus`
2. 本地 HTTPS endpoint 使用自签证书，因此实现里会跳过 TLS 校验
3. 进程识别依赖 marker；如果上游进程参数格式变化，需要同步调整 `CodeiumFamilySpec`

## 与其他项目方案的差异

有些外部项目会直接使用用户 OAuth access token 请求远端接口来判断计划等级。

BananaTray 不依赖这种方式，原因是：

1. BananaTray 不持有用户远端 access token
2. 本地 language server 已能提供足够信息
3. 本地 API + cache fallback 更符合 BananaTray 现有 provider 架构

## 结论

Codeium-family 架构的核心原则是：

- **Antigravity / Windsurf 身份分离**
- **底层 transport / cache / parser 共享**
- **通过 spec 参数化稳定差异**
- **运行时通过 debug helper 做真实环境确认**
