// GJS 集成测试专用 i18n 桩。
//
// quotaClient.js 导入 ./i18n.js，而真正的 i18n.js 依赖
// resource:///org/gnome/shell/extensions/extension.js，只在 GNOME Shell 进程内可用。
// GJS + dbus-run-session 测试环境没有 GNOME Shell resource，因此 orchestrator
// (scripts/test-gnome-extension-gjs.sh) 在临时目录里用本文件覆盖 i18n.js。
//
// 行为与 gnome-shell-extension/tests/mock-i18n.mjs（Node 单测桩）一致：纯 passthrough。

export function _(text) {
    return text;
}

export function ngettext(singular, plural, count) {
    return count === 1 ? singular : plural;
}
