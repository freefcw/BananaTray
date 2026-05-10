// Test-only stubs for gi:// imports used by quotaClient.js.
// Only the module-level code needs GLib/Gio; the exported validation
// functions are pure JS and don't touch GObject introspection at runtime.
//
// The resolve hook maps gi://Gio and gi://GLib to this file.
// Since both use default imports, we rely on the load hook (in hooks.mjs)
// to select the correct default export based on the original specifier.

// Minimal GLib stub
export const GLib = {
    Variant: class Variant {
        constructor() {}
    },
    VariantType: class VariantType {
        constructor() {}
    },
    get_monotonic_time() {
        return Date.now() * 1000;
    },
};

// Minimal Gio stub
export const Gio = {
    BusType: {SESSION: 0},
    BusNameWatcherFlags: {NONE: 0},
    DBusCallFlags: {NONE: 0},
    DBusProxyFlags: {NONE: 0},
    DBusProxy: {
        makeProxyWrapper(_xml) {
            return function StubProxy() {};
        },
    },
    DBus: {session: {}},
    Cancellable: class Cancellable {
        cancel() {}
    },
    bus_watch_name() { return 0; },
    bus_unwatch_name() {},
};
