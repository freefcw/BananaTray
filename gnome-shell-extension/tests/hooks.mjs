// ESM resolve + load hooks: redirects GNOME-only imports to Node.js-compatible mocks.
// Keep hooks synchronous so Node's registerHooks() API can use them without warnings.

const GI_MODULES = new Map([
    ['gi://Gio', 'Gio'],
    ['gi://GLib', 'GLib'],
]);

export function resolve(specifier, context, nextResolve) {
    // Redirect ./i18n.js → mock-i18n.mjs
    if (specifier === './i18n.js' && context.parentURL?.includes('gnome-shell-extension/')) {
        const parentDir = new URL('./', context.parentURL).href;
        const mockUrl = new URL('./tests/mock-i18n.mjs', parentDir).href;
        return nextResolve(mockUrl, context);
    }

    // Redirect gi://Gio, gi://GLib → synthetic module with correct default export.
    // We tag the URL with a query param so the load hook knows which export to pick.
    if (GI_MODULES.has(specifier)) {
        const testsDir = new URL('./', import.meta.url).href;
        const mockUrl = new URL(`./mock-gi.mjs?name=${GI_MODULES.get(specifier)}`, testsDir).href;
        return {url: mockUrl, shortCircuit: true};
    }

    return nextResolve(specifier, context);
}

export function load(url, context, nextLoad) {
    // For gi:// redirects, wrap the mock module to re-export the correct named export as default.
    const parsed = new URL(url);
    const giName = parsed.searchParams.get('name');
    if (giName && parsed.pathname.endsWith('/mock-gi.mjs')) {
        // Strip query to get the real file URL for the import
        const cleanUrl = url.split('?')[0];
        return {
            format: 'module',
            shortCircuit: true,
            source: `export { ${giName} as default } from '${cleanUrl}';`,
        };
    }

    return nextLoad(url, context);
}
