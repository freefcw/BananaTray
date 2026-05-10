// ESM resolve hook: redirects './i18n.js' imports to mock-i18n.mjs.
// Used by register.mjs via module.register().

export function resolve(specifier, context, nextResolve) {
    if (specifier === './i18n.js' && context.parentURL?.includes('gnome-shell-extension/')) {
        const mockUrl = new URL('./tests/mock-i18n.mjs', context.parentURL.replace(/\/[^/]+$/, '/')).href;
        return nextResolve(mockUrl, context);
    }
    return nextResolve(specifier, context);
}
