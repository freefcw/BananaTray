# Claude Provider

Claude provider 支持 OAuth Usage API 与 CLI 两种来源。

## 文件分工

- mod.rs：provider facade，负责 API / CLI source 编排。
- api_probe.rs：OAuth 凭据、token 刷新、HTTP 请求和 User-Agent 协商。
- usage_parser.rs：Usage API 主配额以及 iguana_necktie、cedar_ember 可选额度块的纯解析与校验。
- cli_probe.rs：Claude CLI 输出解析和 CLI 探测。
- credentials.rs：OAuth 凭据读取、刷新和原子写回。
- probe.rs：probe 抽象和 probe mode 定义。

可选额度块解析失败时只丢弃对应额度，不影响同一响应中的会话、周配额；cedar_ember grant 汇总上限为 50，超过上限或数据形态异常时整块忽略。
