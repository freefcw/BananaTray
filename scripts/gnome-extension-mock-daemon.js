#!/usr/bin/env gjs
// BananaTray GNOME Shell Extension mock D-Bus daemon.
//
// Runs inside the nested GNOME Shell D-Bus session used by
// scripts/dev-gnome-extension.sh. It implements the same small D-Bus surface
// that the extension consumes, so GJS UI work does not require a full Rust app.

const {Gio, GLib, GLibUnix} = imports.gi;

const DBUS_ID = 'com.bananatray.Daemon';
const DBUS_PATH = '/com/bananatray/Daemon';
const SCHEMA_VERSION = 1;

const DBUS_INTERFACE_XML = `
<node>
  <interface name="com.bananatray.Daemon">
    <method name="GetAllQuotas">
      <arg name="json_data" type="s" direction="out"/>
    </method>
    <method name="RefreshAll">
      <arg name="json_data" type="s" direction="out"/>
    </method>
    <method name="OpenSettings"/>
    <signal name="RefreshComplete">
      <arg name="json_data" type="s"/>
    </signal>
    <property name="IsActive" type="b" access="read"/>
  </interface>
</node>`;

const STATUS_ROTATION = ['Green', 'Yellow', 'Red'];

let exportedObject = null;
let busOwnerId = 0;
let refreshCount = 0;

function quota(label, used, limit, statusLevel, displayText, quotaTypeKey, barRatio = null) {
    const ratio = barRatio ?? (limit > 0 ? used / limit : 0);
    return {
        label,
        used,
        limit,
        status_level: statusLevel,
        display_text: displayText,
        bar_ratio: Math.max(0, Math.min(1, ratio)),
        quota_type_key: quotaTypeKey,
    };
}

function provider(id, displayName, worstStatus, quotas, options = {}) {
    return {
        id,
        display_name: displayName,
        icon_asset: `src/icons/provider-${id}.svg`,
        connection: options.connection ?? 'Connected',
        account_email: options.accountEmail ?? `${id}@example.test`,
        account_tier: options.accountTier ?? 'Dev Mock',
        worst_status: worstStatus,
        quotas,
    };
}

function buildSnapshot() {
    const status = STATUS_ROTATION[refreshCount % STATUS_ROTATION.length];
    const claudeSession = Math.min(100, 42 + refreshCount * 3);
    const claudeWeekly = Math.min(100, 58 + refreshCount * 2);
    const ampUsed = Math.min(10, 4 + refreshCount * 0.7);
    const codexRemaining = Math.max(0, 100 - refreshCount * 9);
    const cursorConnection = refreshCount % 4 === 1 ? 'Refreshing' : 'Connected';
    const geminiConnection = refreshCount % 3 === 2 ? 'Disconnected' : 'Error';

    return {
        schema_version: SCHEMA_VERSION,
        header: {
            status_text: `Mock refresh #${refreshCount}`,
            status_kind: 'Synced',
        },
        providers: [
            provider('claude', 'Claude', status, [
                quota('Session', claudeSession, 100, status, `${claudeSession}%`, 'session'),
                quota('Weekly', claudeWeekly, 100, claudeWeekly >= 90 ? 'Red' : 'Yellow', `${100 - claudeWeekly}% left`, 'weekly', (100 - claudeWeekly) / 100),
            ]),
            provider('codex', 'Codex', codexRemaining <= 20 ? 'Red' : 'Green', [
                quota('Daily', 100 - codexRemaining, 100, codexRemaining <= 20 ? 'Red' : 'Green', `${codexRemaining}% left`, 'daily', codexRemaining / 100),
                quota('Monthly', 28 + refreshCount, 100, 'Green', `${28 + refreshCount}%`, 'monthly'),
            ], {
                accountEmail: 'codex@example.test',
                accountTier: 'Team',
            }),
            provider('amp', 'Amp', ampUsed >= 9 ? 'Red' : 'Yellow', [
                quota('Credits', ampUsed, 10, ampUsed >= 9 ? 'Red' : 'Yellow', `${(10 - ampUsed).toFixed(1)} left`, 'credits', (10 - ampUsed) / 10),
            ], {
                connection: refreshCount % 5 === 3 ? 'Error' : 'Connected',
                accountEmail: null,
                accountTier: 'Credits',
            }),
            provider('cursor', 'Cursor', cursorConnection === 'Refreshing' ? 'Yellow' : 'Green', [
                quota('Fast Requests', 35, 100, 'Green', '65% left', 'fast_requests', 0.65),
            ], {
                connection: cursorConnection,
                accountTier: 'Pro',
            }),
            provider('gemini', 'Gemini', 'Red', [], {
                connection: geminiConnection,
                accountEmail: null,
                accountTier: null,
            }),
        ],
    };
}

function snapshotJson() {
    return JSON.stringify(buildSnapshot());
}

function emitRefreshComplete(jsonData) {
    if (!exportedObject)
        return;

    exportedObject.emit_signal('RefreshComplete', GLib.Variant.new('(s)', [jsonData]));
}

class BananaTrayMockDaemon {
    get IsActive() {
        return true;
    }

    GetAllQuotas() {
        return snapshotJson();
    }

    RefreshAll() {
        refreshCount += 1;
        const jsonData = snapshotJson();
        emitRefreshComplete(jsonData);
        return jsonData;
    }

    OpenSettings() {
        print('BananaTray mock daemon: OpenSettings called');
    }
}

const loop = new GLib.MainLoop(null, false);

function shutdown() {
    if (exportedObject) {
        exportedObject.unexport();
        exportedObject = null;
    }
    if (busOwnerId !== 0) {
        Gio.bus_unown_name(busOwnerId);
        busOwnerId = 0;
    }
    loop.quit();
}

GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, 15, () => {
    shutdown();
    return GLib.SOURCE_REMOVE;
});

busOwnerId = Gio.bus_own_name(
    Gio.BusType.SESSION,
    DBUS_ID,
    Gio.BusNameOwnerFlags.NONE,
    connection => {
        exportedObject = Gio.DBusExportedObject.wrapJSObject(
            DBUS_INTERFACE_XML,
            new BananaTrayMockDaemon()
        );
        exportedObject.export(connection, DBUS_PATH);
        print(`BananaTray mock daemon registered: ${DBUS_ID}`);
        emitRefreshComplete(snapshotJson());
    },
    null,
    () => {
        print(`BananaTray mock daemon lost bus name: ${DBUS_ID}`);
        shutdown();
    }
);

GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 5, () => {
    refreshCount += 1;
    emitRefreshComplete(snapshotJson());
    return GLib.SOURCE_CONTINUE;
});

loop.run();
