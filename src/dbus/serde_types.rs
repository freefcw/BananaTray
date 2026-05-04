//! D-Bus JSON DTO — 从 `application::selectors::dbus_dto` re-export
//!
//! DTO 类型和格式化函数定义在 `application::selectors::dbus_dto`，
//! 在该模块可全平台编译和测试。此处仅做 re-export 保持 `dbus` 模块接口不变。

pub use crate::application::{
    DBusHeaderInfo, DBusProviderEntry, DBusQuotaEntry, DBusQuotaSnapshot,
};
