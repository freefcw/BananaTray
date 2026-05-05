// Extension 本地 gettext 包装，所有 GNOME Shell UI 文案统一从这里取翻译。

import {
    gettext as extensionGettext,
    ngettext as extensionNgettext,
} from 'resource:///org/gnome/shell/extensions/extension.js';

export function _(text) {
    return extensionGettext(text);
}

export function ngettext(singular, plural, count) {
    return extensionNgettext(singular, plural, count);
}
