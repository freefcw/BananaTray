---
name: issue-status-audit
description: "审计 GitHub Issues 在代码库中的实现情况，自动关闭已完成的 Issue 并为部分完成的 Issue 添加进展说明。Use when checking issue implementation status, closing completed issues, or auditing open issues against codebase."
---

# Issue 实现状态审计

检查 GitHub Issues 在当前代码库中的实现情况，自动关闭已完成的 Issue 并对部分实现的 Issue 添加进展评论。

## 工作流程

### Step 1: 获取 Issue 列表

```bash
gh issue list --repo <owner>/<repo>
```

逐个获取 Issue 详情以了解具体需求：

```bash
gh issue view <number> --repo <owner>/<repo>
```

### Step 2: 逐个检查实现状态

对每个 Issue，使用 `finder` 工具在代码库中搜索相关功能的实现情况。搜索时应：

- 根据 Issue 描述提炼关键功能点
- 搜索相关模块、函数、组件
- 判断核心功能是否已实现

### Step 3: 分类汇总

将所有 Issue 分为三类，以表格形式输出：

| 分类 | 说明 |
|------|------|
| ✅ 已实现 | 核心功能已完整实现 |
| ⚠️ 部分实现 | 有基础实现但未完全满足需求 |
| ❌ 未实现 | 无相关代码或仅有占位 |

### Step 4: 关闭已实现的 Issue

对 ✅ 已实现的 Issue 执行关闭并附带说明：

```bash
gh issue close <number> --repo <owner>/<repo> --comment "已实现：<具体实现说明，包含关键文件路径>"
```

**说明要求：**
- 列出核心实现文件路径
- 简述实现方式

### Step 5: 为部分实现的 Issue 添加评论

对 ⚠️ 部分实现的 Issue 添加进展说明：

```bash
gh issue comment <number> --repo <owner>/<repo> --body "当前进展：<已实现的部分说明>。待完成：<尚未满足的需求>。"
```

**评论要求：**
- 说明已实现的部分及相关文件
- 明确指出与 Issue 需求的差距

## 注意事项

- 关闭前必须确认功能**确实已完整实现**，有疑问时归为"部分实现"
- 所有操作前先向用户汇总分类结果，经确认后再批量执行
- 网络请求失败时自动重试
