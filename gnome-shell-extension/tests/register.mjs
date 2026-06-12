// Register ESM resolve hooks before any test imports.
// Usage: node --import ./gnome-shell-extension/tests/register.mjs --test ...
import * as moduleApi from 'node:module';

if (typeof moduleApi.registerHooks === 'function') {
    const hooks = await import('./hooks.mjs');
    moduleApi.registerHooks(hooks);
} else {
    moduleApi.register('./hooks.mjs', import.meta.url);
}
