// Test-only mock for i18n.js.
// In GNOME Shell, _() and ngettext() come from the extension gettext API.
// In Node.js tests, we use passthrough implementations.

export function _(text) {
    return text;
}

export function ngettext(singular, plural, count) {
    return count === 1 ? singular : plural;
}
