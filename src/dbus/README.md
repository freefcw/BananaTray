# src/dbus/

D-Bus 服务模块，提供 `com.bananatray.Daemon` 接口供 GNOME Shell Extension 查询配额数据。

仅 Linux + `app` feature 下编译（`cfg(target_os = "linux")` + `cfg(feature = "app")`）。

正式 Linux 安装包会安装 Session D-Bus activation 文件
`/usr/share/dbus-1/services/com.bananatray.Daemon.service` 和 systemd user unit
`/usr/lib/systemd/user/bananatray.service`。Extension 启动或用户主动刷新/打开设置时会异步请求
`StartServiceByName("com.bananatray.Daemon")`，由 Session Bus 拉起安装后的 `bananatray` 二进制。
AppImage 不安装到宿主 D-Bus 搜索路径，因此不提供 activation。

## 架构概览

```
主线程 (GPUI)                          D-Bus 线程 (dbus-service)
  │                                       │
  ├─ snapshot_cache.update(json) ──────>  ├─ GetAllQuotas 读取 snapshot_cache
  ├─ signal_tx.send(json) ─────────────>  ├─ RefreshAll 读取 snapshot_cache + 通知 GPUI
  │                                       ├─ ObjectServer 处理方法调用
  │ <── action_rx.recv() ────────────── │
  └─ dispatch_in_app() 处理               └─ iface_ref.refresh_complete(json)
     OpenSettings / RefreshAll            └─ smol 异步执行器
```

**线程模型**：2 线程

1. **D-Bus 线程**（`dbus-service`）：运行 `smol::block_on` 执行器，持有 zbus `ObjectServer`，处理 D-Bus 方法调用和信号发射。
2. **GPUI 主线程**：通过 foreground executor 的 `spawn()` 消费 `action_rx`，执行需要 GPUI 上下文的操作（`OpenSettings`、`RefreshAll`）。

### 为什么不用 Rc<RefCell<AppState>>

zbus 5 的 `Interface` trait 要求 `Send + Sync`，而 `Rc<RefCell<_>>` 不满足。因此 `BananaTrayIface` 不持有 `AppState`，而是持有：

- `Arc<Mutex<String>>` — 缓存的快照 JSON（GPUI 主线程写入，D-Bus 线程读取）
- `smol::channel::Sender` — 动作请求通道（D-Bus → GPUI 主线程）

`Rc<RefCell<AppState>>` 仍由 `spawn_action_bridge` 持有，但只在 GPUI 主线程上使用（通过 `async_cx.update()`），不会 move 到 D-Bus 线程。

## 文件说明

| 文件 | 职责 |
|------|------|
| `mod.rs` | 模块入口，公开 `DBusServiceHandle` 和 DTO re-export；`start_dbus_service()` 启动服务；`spawn_action_bridge()` 桥接 action 到 GPUI 主线程；`run_dbus_server()` 在独立线程运行 zbus 服务 |
| `iface.rs` | zbus `#[interface]` 实现，定义 D-Bus 方法/信号/属性；`DBusActionRequest` 枚举表示 D-Bus → GPUI 的动作请求（含 `OpenSettings` 和 `RefreshAll`） |
| `serde_types.rs` | 纯 re-export 文件，从 `application::selectors::dbus_dto` 导出 DTO 类型和格式化函数 |

## D-Bus 接口契约

**Bus 名称**：`com.bananatray.Daemon`
**Object 路径**：`/com/bananatray/Daemon`

### 方法

| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `GetAllQuotas` | — | `s` (JSON) | 从缓存读取配额快照 |
| `RefreshAll` | — | `s` (JSON) | 通知 GPUI 主线程发起刷新 + 返回当前缓存快照 |
| `OpenSettings` | — | — | 请求打开设置窗口（异步，在 GPUI 主线程执行） |

### 信号

| 信号 | 参数 | 说明 |
|------|------|------|
| `RefreshComplete` | `s` (JSON) | 刷新完成后发射，携带完整配额快照 |

### 属性

| 属性 | 类型 | 访问 | 说明 |
|------|------|------|------|
| `IsActive` | `b` | read | 始终为 `true`，表示 daemon 正在运行 |

## JSON 快照格式

方法返回值和信号参数都是 `DBusQuotaSnapshot` 的 JSON 序列化：

```json
{
  "schema_version": 1,
  "header": {
    "status_text": "Synced",
    "status_kind": "Synced"
  },
  "providers": [
    {
      "id": "claude",
      "display_name": "Claude",
      "icon_asset": "claude.svg",
      "connection": "Connected",
      "account_email": "user@example.com",
      "account_tier": "Pro",
      "worst_status": "Green",
      "quotas": [
        {
          "label": "Session",
          "used": 45.0,
          "limit": 100.0,
          "status_level": "Green",
          "display_text": "45%",
          "bar_ratio": 0.45,
          "quota_type_key": "session"
        }
      ]
    }
  ]
}
```

DTO 类型和格式化函数定义在 `application::selectors::dbus_dto`（跨平台可测试），本模块仅做 re-export。

### JSON 兼容规则

- `schema_version` 是必填字段。当前版本为 `1`。
- 同一个 `schema_version` 内允许新增字段；GNOME Shell Extension 必须忽略未知字段。
- 删除字段、字段改名、字段类型变化、枚举字符串语义变化，都必须提升 `schema_version`。
- Extension 当前只接受 `schema_version == 1`，并在渲染前校验最小必填字段：
  - 顶层：`schema_version`、`header`、`providers`
  - `header`：`status_text`、`status_kind`
  - provider：`id`、`display_name`、`icon_asset`、`connection`、`worst_status`、`quotas`
  - quota：`label`、`used`、`limit`、`status_level`、`display_text`、`quota_type_key`

`quota.bar_ratio` 是 schema v1 内新增的可选字段，表示 Overview 进度条比例 `[0.0, 1.0]`。
它的语义与当前 `quota_display_mode` 对齐：Remaining 模式表示剩余比例，Used 模式表示已用比例。GNOME Shell Extension 会优先使用该字段；旧 daemon 未提供时会降级为 `used / limit`。

## 数据流

### GetAllQuotas（读取缓存）

1. GNOME Shell Extension 调用 `GetAllQuotas`
2. `BananaTrayIface::get_all_quotas()` 从 `Arc<Mutex<String>>` 缓存读取
3. 返回 JSON（无需跨线程通信）

### RefreshAll（通知 + 缓存读取）

1. GNOME Shell Extension 调用 `RefreshAll`
2. `BananaTrayIface::refresh_all()` 通过 `action_tx` 发送 `RefreshAll` 请求 + 返回缓存快照
3. GPUI 主线程 `spawn_action_bridge` 收到请求 → `dispatch_in_app(AppAction::RefreshAll)`
4. 刷新完成后，事件泵更新缓存并发射 `RefreshComplete` 信号

### RefreshComplete 信号（缓存更新 + 信号发射）

1. 后台 `RefreshCoordinator` 完成刷新 → 发送 `RefreshEvent`
2. 事件泵收到 `RefreshEventReceived` → reducer 更新 `AppState`
3. `emit_dbus_signals()` 构建 `DBusQuotaSnapshot` → `DBusServiceHandle::emit_refresh_complete()`
4. `emit_refresh_complete()` 更新 `Arc<Mutex<String>>` 缓存 + 通过 `signal_tx` 通知 D-Bus 线程
5. D-Bus 线程通过 `InterfaceRef::refresh_complete()` 发射 zbus 信号

## 关键设计决策

- **Iface 不持有 AppState**：zbus `Interface` 要求 `Send + Sync`，`Rc<RefCell<_>>` 不满足。改用 `Arc<Mutex<String>>` 缓存 + channel 通信。
- **信号发射通过 InterfaceRef**：zbus 5 的 signal 方法生成 `<Name>Signals` trait，为 `InterfaceRef<Name>` 和 `SignalEmitter` 实现。通过 `conn.object_server().interface()` 获取 `InterfaceRef`，调用其 signal 方法。
- **DTO 放在 `application/selectors/` 而非 `dbus/`**：DTO 类型和格式化逻辑不依赖 GPUI/zbus，可在任何平台编译和测试。
- **action bridge 复用 GPUI foreground executor**：避免为转发 `action_rx` 创建额外线程。`Rc<RefCell<AppState>>` 只在 GPUI 主线程上使用。
- **信号通道用 `smol::channel::bounded`**：与 zbus 的 async-io 运行时兼容，避免跨运行时桥接。
