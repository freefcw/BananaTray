# Custom Provider（YAML 声明式 Provider）

允许用户通过 YAML 文件声明自定义 Provider，无需编写 Rust 代码。

## 使用方法

将 YAML 文件放到 `~/.config/bananatray/providers/` 目录（macOS 上是 `~/Library/Application Support/bananatray/providers/`），应用启动时自动加载。

示例文件见 `examples/custom-provider-cli.yaml` 和 `examples/custom-provider-http.yaml`。

## 模块结构

```
custom/
  mod.rs          — 模块入口，re-export
  schema.rs       — YAML 反序列化结构体
  fetcher.rs      — 数据获取（CLI / HTTP GET / HTTP POST）
  extractor.rs    — 响应解析（JSON 路径提取 / 正则匹配）
  provider.rs     — CustomProvider（impl AiProvider）
  loader.rs       — 文件扫描 + 加载 + 校验
```

## 设计原则

- **SRP**: 每个模块职责单一（schema 定义 / 获取 / 解析 / Provider 实现 / 文件加载）
- **OCP**: 新增自定义 Provider 只需添加 YAML 文件，不修改任何 Rust 代码
- **DIP**: CustomProvider 依赖 fetcher/extractor 的函数接口，不依赖具体实现细节

## 支持的数据获取方式

| type       | 说明 |
|------------|------|
| `cli`      | 执行 CLI 命令，获取 stdout/stderr |
| `http_get` | HTTP GET 请求 |
| `http_post`| HTTP POST 请求（JSON body） |

## 支持的认证方式

| auth type     | 说明 |
|---------------|------|
| `bearer_env`  | 从环境变量读取 token，设置 `Authorization: Bearer {token}` |
| `header_env`  | 从环境变量读取值，设置自定义 header |

## 支持的解析方式

| format  | 说明 |
|---------|------|
| `json`  | 点分路径提取（如 `data.usage.used`），支持数组索引（如 `items.0.value`） |
| `regex` | 正则 capture group 提取 used/limit 值 |
