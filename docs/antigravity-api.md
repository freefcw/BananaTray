# Antigravity Provider — API 分析与订阅等级识别

## 概述

Antigravity（前身 Codeium/Windsurf）provider 通过本地 language server 进程的 gRPC-over-HTTP API 获取用户配额信息。

## 数据获取流程

```
1. pgrep 检测 language_server_macos_arm 进程
2. 从进程参数提取 --csrf_token 和 --extension_server_port
3. lsof 发现进程的 TCP LISTEN 端口
4. POST https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus
   Headers:
     Content-Type: application/json
     X-Codeium-Csrf-Token: {csrf_token}
     Connect-Protocol-Version: 1
   Body: {"metadata":{"ideName":"antigravity"}}
```

## GetUserStatus 响应结构（关键字段）

```json
{
  "userStatus": {
    "name": "...",
    "email": "user@example.com",

    "userTier": {                              // ✅ 正确的订阅等级来源
      "id": "g1-ultra-tier",                   // "g1-ultra-tier" | "g1-pro-tier" | ...
      "name": "Google AI Ultra",               // "Google AI Ultra" | "Google AI Pro" | ...
      "description": "Google AI Ultra",
      "upgradeSubscriptionText": "You are subscribed to the best plan.",
      "availableCredits": [
        {
          "creditType": "GOOGLE_ONE_AI",
          "creditAmount": "25043",
          "minimumCreditAmountForUsage": "50"
        }
      ]
    },

    "planStatus": {
      "planInfo": {
        "planName": "Pro",                     // ⚠️ 不可靠！Pro 和 Ultra 都返回 "Pro"
        "teamsTier": "TEAMS_TIER_PRO",         // ⚠️ 不可靠！Pro 和 Ultra 都返回相同值
        "monthlyPromptCredits": 50000,
        "monthlyFlowCredits": 150000,
        ...
      },
      "availablePromptCredits": 500,
      "availableFlowCredits": 100
    },

    "cascadeModelConfigData": {
      "clientModelConfigs": [
        {
          "label": "Claude Opus 4.6 (Thinking)",
          "quotaInfo": {
            "remainingFraction": 1.0,          // 0.0 ~ 1.0，剩余配额比例
            "resetTime": "2026-04-01T07:42:12Z"
          },
          "allowedTiers": [
            "TEAMS_TIER_PRO",
            "TEAMS_TIER_TEAMS",
            "TEAMS_TIER_PRO_ULTIMATE",
            ...
          ]
        }
      ]
    }
  }
}
```

## 订阅等级识别问题（2026-04 修复）

### 问题

`planStatus.planInfo.planName` 对 Pro 和 Ultra 用户都返回 `"Pro"`，导致 Ultra 用户被错误显示为 Pro。

### 根因分析

| 字段路径 | Pro 用户返回 | Ultra 用户返回 | 能否区分 |
|----------|-------------|---------------|---------|
| `planStatus.planInfo.planName` | `"Pro"` | `"Pro"` | ❌ |
| `planStatus.planInfo.teamsTier` | `"TEAMS_TIER_PRO"` | `"TEAMS_TIER_PRO"` | ❌ |
| **`userTier.id`** | `"g1-pro-tier"` | `"g1-ultra-tier"` | ✅ |
| **`userTier.name`** | `"Google AI Pro"` | `"Google AI Ultra"` | ✅ |

### 解决方案

在 `parse_user_status()` 中，优先从 `userTier.name` 获取计划名称，回退到 `planInfo.planName`：

```rust
let plan_name = user_status
    .pointer("/userTier/name")
    .and_then(|v| v.as_str())
    .or_else(|| {
        user_status
            .pointer("/planStatus/planInfo/planName")
            .and_then(|v| v.as_str())
    })
    .map(|s| s.to_string());
```

### 已知 userTier.id 值

| id | name | 说明 |
|----|------|------|
| `g1-pro-tier` | Google AI Pro | Pro 计划 |
| `g1-ultra-tier` | Google AI Ultra | Ultra 计划（最高级） |

## 与 Antigravity-Manager 项目的对比

[Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager) 使用了另一种方式获取订阅等级：

```
POST https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:loadCodeAssist
Authorization: Bearer {access_token}
Body: {"metadata": {"ideType": "ANTIGRAVITY"}}

响应中 paidTier.id 返回 "FREE" | "PRO" | "ULTRA"
```

该方式需要用户的 OAuth access_token，BananaTray 不采用此方式，因为：
1. BananaTray 不持有用户的 access_token（通过本地 language server 间接通信）
2. 本地 `GetUserStatus` API 的 `userTier` 字段已经能正确区分

## 配额数据提取

每个模型的配额通过 `cascadeModelConfigData.clientModelConfigs` 获取：

- `label`: 模型显示名（如 "Claude Opus 4.6 (Thinking)"）
- `quotaInfo.remainingFraction`: 剩余配额比例（0.0 ~ 1.0）
- `quotaInfo.resetTime`: ISO 8601 格式的重置时间

转换公式：`used_percent = (1.0 - remainingFraction) * 100.0`

## 连接细节

- HTTPS 优先（本地自签证书，需跳过 TLS 验证），失败回退 HTTP
- 支持 extension_server_port 作为备用端口
- 进程检测支持 Intel (`language_server_macos`) 和 ARM (`language_server_macos_arm`)
