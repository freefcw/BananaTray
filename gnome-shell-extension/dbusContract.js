// schema v1 中由 Rust producer 发出的稳定 wire 值。
// 修改此集合必须同步更新 tests/fixtures/dbus-v1-wire.json；跨语言契约门禁会校验漂移。
export const STATUS_KIND_WIRE_VALUES = Object.freeze([
    'Synced',
    'Syncing',
    'Stale',
    'Offline',
]);
