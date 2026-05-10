// Register ESM resolve hooks before any test imports.
// Usage: node --import ./gnome-shell-extension/tests/register.mjs --test ...
import { register } from 'node:module';

register('./hooks.mjs', import.meta.url);
