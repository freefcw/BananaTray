// quotaClient.js schema validation 单元测试
//
// 运行: node --import ./gnome-shell-extension/tests/register.mjs \
//            --test ./gnome-shell-extension/tests/quotaClient.test.mjs

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
    validateSnapshot,
    parseSnapshot,
    SUPPORTED_SCHEMA_VERSION,
    QuotaClient,
} from '../quotaClient.js';

// -- Helpers --

function makeQuota(overrides = {}) {
    return {
        label: 'Requests',
        used: 50,
        limit: 100,
        status_level: 'Green',
        display_text: '50 / 100',
        quota_type_key: 'requests',
        ...overrides,
    };
}

function makeProvider(overrides = {}) {
    return {
        id: 'test',
        display_name: 'Test Provider',
        icon_asset: 'test.svg',
        connection: 'Connected',
        account_email: null,
        account_tier: null,
        worst_status: 'Green',
        quotas: [],
        ...overrides,
    };
}

function makeSnapshot(overrides = {}) {
    return {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        header: {
            status_text: 'Synced',
            status_kind: 'Synced',
        },
        providers: [],
        ...overrides,
    };
}

async function flushPromises() {
    await Promise.resolve();
    await Promise.resolve();
}

// ============================================================
// validateSnapshot — happy path
// ============================================================
describe('validateSnapshot — valid data', () => {
    it('accepts a minimal valid snapshot', () => {
        const snapshot = makeSnapshot();
        const result = validateSnapshot(snapshot);
        assert.strictEqual(result, snapshot);
    });

    it('accepts snapshot with providers and quotas', () => {
        const snapshot = makeSnapshot({
            providers: [
                makeProvider({
                    quotas: [makeQuota(), makeQuota({label: 'Tokens'})],
                }),
                makeProvider({id: 'other', display_name: 'Other'}),
            ],
        });
        const result = validateSnapshot(snapshot);
        assert.strictEqual(result.providers.length, 2);
    });

    it('accepts optional account_email and account_tier as null', () => {
        const snapshot = makeSnapshot({
            providers: [makeProvider({account_email: null, account_tier: null})],
        });
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });

    it('accepts optional account_email and account_tier as strings', () => {
        const snapshot = makeSnapshot({
            providers: [makeProvider({account_email: 'a@b.com', account_tier: 'Pro'})],
        });
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });

    it('accepts optional account_email and account_tier as undefined', () => {
        const snapshot = makeSnapshot({
            providers: [makeProvider({account_email: undefined, account_tier: undefined})],
        });
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });
});

// ============================================================
// validateSnapshot — schema_version
// ============================================================
describe('validateSnapshot — schema_version', () => {
    it('rejects missing schema_version', () => {
        const data = makeSnapshot();
        delete data.schema_version;
        assert.throws(() => validateSnapshot(data), /schema_version/);
    });

    it('rejects wrong schema_version', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({schema_version: 999})),
            /unsupported schema_version 999/
        );
    });

    it('rejects schema_version 0', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({schema_version: 0})),
            /unsupported schema_version/
        );
    });
});

// ============================================================
// validateSnapshot — top-level structure
// ============================================================
describe('validateSnapshot — top-level structure', () => {
    it('rejects non-object snapshot', () => {
        assert.throws(() => validateSnapshot(null), /not an object/);
        assert.throws(() => validateSnapshot('string'), /not an object/);
        assert.throws(() => validateSnapshot(42), /not an object/);
    });

    it('rejects array as snapshot', () => {
        assert.throws(() => validateSnapshot([]), /not an object/);
    });

    it('rejects missing header', () => {
        const data = makeSnapshot();
        delete data.header;
        assert.throws(() => validateSnapshot(data), /header is not an object/);
    });

    it('rejects non-object header', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({header: 'string'})),
            /header is not an object/
        );
    });

    it('rejects missing header.status_text', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({header: {status_kind: 'Synced'}})),
            /header missing string status_text/
        );
    });

    it('rejects missing header.status_kind', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({header: {status_text: 'ok'}})),
            /header missing string status_kind/
        );
    });

    it('accepts optional header.elapsed_secs', () => {
        const snapshot = makeSnapshot({
            header: {
                status_text: '10 min ago',
                status_kind: 'Stale',
                elapsed_secs: 600,
            },
        });
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });

    it('rejects non-array providers', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({providers: 'not array'})),
            /providers must be an array/
        );
    });
});

// ============================================================
// validateSnapshot — provider validation
// ============================================================
describe('validateSnapshot — provider validation', () => {
    it('rejects non-object provider', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({providers: ['string']})),
            /provider #0 is not an object/
        );
    });

    for (const field of ['id', 'display_name', 'icon_asset', 'connection', 'worst_status']) {
        it(`rejects provider missing ${field}`, () => {
            const provider = makeProvider();
            delete provider[field];
            assert.throws(
                () => validateSnapshot(makeSnapshot({providers: [provider]})),
                new RegExp(`provider #0 missing string ${field}`)
            );
        });
    }

    it('rejects non-array provider.quotas', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({
                providers: [makeProvider({quotas: 'not array'})],
            })),
            /quotas must be an array/
        );
    });

    it('rejects non-string account_email (when not null/undefined)', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({
                providers: [makeProvider({account_email: 123})],
            })),
            /account_email must be string or null/
        );
    });

    it('rejects non-string account_tier (when not null/undefined)', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({
                providers: [makeProvider({account_tier: true})],
            })),
            /account_tier must be string or null/
        );
    });
});

// ============================================================
// validateSnapshot — quota validation
// ============================================================
describe('validateSnapshot — quota validation', () => {
    it('rejects non-object quota', () => {
        assert.throws(
            () => validateSnapshot(makeSnapshot({
                providers: [makeProvider({quotas: [42]})],
            })),
            /quota #0 is not an object/
        );
    });

    for (const field of ['label', 'status_level', 'display_text', 'quota_type_key']) {
        it(`rejects quota missing string ${field}`, () => {
            const quota = makeQuota();
            delete quota[field];
            assert.throws(
                () => validateSnapshot(makeSnapshot({
                    providers: [makeProvider({quotas: [quota]})],
                })),
                new RegExp(`missing string ${field}`)
            );
        });
    }

    for (const field of ['used', 'limit']) {
        it(`rejects quota missing number ${field}`, () => {
            const quota = makeQuota();
            delete quota[field];
            assert.throws(
                () => validateSnapshot(makeSnapshot({
                    providers: [makeProvider({quotas: [quota]})],
                })),
                new RegExp(`missing number ${field}`)
            );
        });

        it(`rejects quota with non-number ${field}`, () => {
            const quota = makeQuota({[field]: 'string'});
            assert.throws(
                () => validateSnapshot(makeSnapshot({
                    providers: [makeProvider({quotas: [quota]})],
                })),
                new RegExp(`missing number ${field}`)
            );
        });
    }
});

// ============================================================
// validateSnapshot — unknown enum warnings
// ============================================================
describe('validateSnapshot — unknown enum warnings', () => {
    it('warns on unknown status_level', () => {
        const warnings = [];
        const snapshot = makeSnapshot({
            providers: [makeProvider({
                quotas: [makeQuota({status_level: 'purple'})],
            })],
        });
        validateSnapshot(snapshot, msg => warnings.push(msg));
        assert.ok(warnings.some(w => w.includes('status_level') && w.includes('purple')));
    });

    it('warns on unknown connection value', () => {
        const warnings = [];
        const snapshot = makeSnapshot({
            providers: [makeProvider({connection: 'sleeping'})],
        });
        validateSnapshot(snapshot, msg => warnings.push(msg));
        assert.ok(warnings.some(w => w.includes('connection') && w.includes('sleeping')));
    });

    it('warns on unknown worst_status value', () => {
        const warnings = [];
        const snapshot = makeSnapshot({
            providers: [makeProvider({worst_status: 'blue'})],
        });
        validateSnapshot(snapshot, msg => warnings.push(msg));
        assert.ok(warnings.some(w => w.includes('worst_status') && w.includes('blue')));
    });

    it('does not warn on known enum values', () => {
        const warnings = [];
        const snapshot = makeSnapshot({
            providers: [makeProvider({
                connection: 'Connected',
                worst_status: 'Green',
                quotas: [makeQuota({status_level: 'Red'})],
            })],
        });
        validateSnapshot(snapshot, msg => warnings.push(msg));
        assert.strictEqual(warnings.length, 0);
    });
});

// ============================================================
// validateSnapshot — ignores unknown fields
// ============================================================
describe('validateSnapshot — unknown fields tolerance', () => {
    it('ignores extra top-level fields', () => {
        const snapshot = makeSnapshot({extra_field: 'hello'});
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });

    it('ignores extra provider fields', () => {
        const snapshot = makeSnapshot({
            providers: [makeProvider({custom_data: {x: 1}})],
        });
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });

    it('ignores extra quota fields (e.g. bar_ratio)', () => {
        const snapshot = makeSnapshot({
            providers: [makeProvider({
                quotas: [makeQuota({bar_ratio: 0.5, extra: true})],
            })],
        });
        assert.doesNotThrow(() => validateSnapshot(snapshot));
    });
});

// ============================================================
// parseSnapshot
// ============================================================
describe('parseSnapshot', () => {
    it('parses valid JSON and returns validated snapshot', () => {
        const snapshot = makeSnapshot({providers: [makeProvider()]});
        const json = JSON.stringify(snapshot);
        const result = parseSnapshot(json);
        assert.strictEqual(result.schema_version, SUPPORTED_SCHEMA_VERSION);
        assert.strictEqual(result.providers.length, 1);
    });

    it('throws on invalid JSON', () => {
        assert.throws(() => parseSnapshot('{bad json'), /JSON/i);
    });

    it('throws on valid JSON but invalid snapshot', () => {
        assert.throws(() => parseSnapshot('{"foo": 1}'), /schema_version/);
    });

    it('collects warnings via callback', () => {
        const warnings = [];
        const snapshot = makeSnapshot({
            providers: [makeProvider({connection: 'unknown_state'})],
        });
        parseSnapshot(JSON.stringify(snapshot), msg => warnings.push(msg));
        assert.ok(warnings.length > 0);
    });
});

// ============================================================
// QuotaClient — pending proxy actions
// ============================================================
describe('QuotaClient — pending proxy actions', () => {
    it('replays OpenSettings after activation creates a proxy', async () => {
        const proxyCalls = [];
        const errors = [];
        const client = new QuotaClient({
            onError: (logMessage, uiMessage) => errors.push({logMessage, uiMessage}),
        });
        client._requestDaemonActivation = () => true;

        assert.equal(await client.openSettings(), true);

        client._onProxyReady({
            connectSignal: () => 1,
            OpenSettingsAsync: async () => proxyCalls.push('openSettings'),
            GetAllQuotasAsync: async () => [JSON.stringify(makeSnapshot())],
        });
        await flushPromises();

        assert.deepEqual(proxyCalls, ['openSettings']);
        assert.equal(errors.length, 0);
    });

    it('replays RefreshAll after activation creates a proxy', async () => {
        const snapshots = [];
        const proxyCalls = [];
        const client = new QuotaClient({
            onSnapshot: snapshot => snapshots.push(snapshot),
        });
        client._requestDaemonActivation = () => true;

        assert.equal(await client.refreshAll(), true);

        client._onProxyReady({
            connectSignal: () => 1,
            RefreshAllAsync: async () => {
                proxyCalls.push('refreshAll');
                return [JSON.stringify(makeSnapshot({providers: [makeProvider()]}))];
            },
            GetAllQuotasAsync: async () => {
                proxyCalls.push('getAllQuotas');
                return [JSON.stringify(makeSnapshot())];
            },
        });
        await flushPromises();

        // refresh 已在待办队列里 → 回放 refreshAll 拉取快照，跳过默认 fetch
        assert.deepEqual(proxyCalls, ['refreshAll']);
        assert.equal(snapshots.length, 1);
        assert.equal(snapshots[0].providers.length, 1);
    });

    it('auto-fetches when proxy becomes ready with no pending actions', async () => {
        const snapshots = [];
        const proxyCalls = [];
        const client = new QuotaClient({
            onSnapshot: snapshot => snapshots.push(snapshot),
        });

        client._onProxyReady({
            connectSignal: () => 1,
            GetAllQuotasAsync: async () => {
                proxyCalls.push('getAllQuotas');
                return [JSON.stringify(makeSnapshot({providers: [makeProvider()]}))];
            },
            RefreshAllAsync: async () => {
                proxyCalls.push('refreshAll');
                return [JSON.stringify(makeSnapshot())];
            },
        });
        await flushPromises();

        // 无待办动作 → 自动补一次 fetch 填充弹窗初始数据
        assert.deepEqual(proxyCalls, ['getAllQuotas']);
        assert.equal(snapshots.length, 1);
    });

    it('replays OpenSettings and still auto-fetches (no refresh pending)', async () => {
        const snapshots = [];
        const proxyCalls = [];
        const client = new QuotaClient({
            onSnapshot: snapshot => snapshots.push(snapshot),
        });
        client._requestDaemonActivation = () => true;

        await client.openSettings();

        client._onProxyReady({
            connectSignal: () => 1,
            OpenSettingsAsync: async () => proxyCalls.push('openSettings'),
            GetAllQuotasAsync: async () => {
                proxyCalls.push('getAllQuotas');
                return [JSON.stringify(makeSnapshot())];
            },
            RefreshAllAsync: async () => {
                proxyCalls.push('refreshAll');
                return [JSON.stringify(makeSnapshot())];
            },
        });
        await flushPromises();

        // 待办只有 openSettings（非 refresh）→ 回放 openSettings + 补 fetch
        assert.deepEqual(proxyCalls, ['openSettings', 'getAllQuotas']);
        assert.equal(snapshots.length, 1);
    });

    it('replays both refresh and openSettings when both pending (no auto-fetch)', async () => {
        const snapshots = [];
        const proxyCalls = [];
        const client = new QuotaClient({
            onSnapshot: snapshot => snapshots.push(snapshot),
        });
        client._requestDaemonActivation = () => true;

        await client.refreshAll();
        await client.openSettings();

        client._onProxyReady({
            connectSignal: () => 1,
            OpenSettingsAsync: async () => proxyCalls.push('openSettings'),
            GetAllQuotasAsync: async () => {
                proxyCalls.push('getAllQuotas');
                return [JSON.stringify(makeSnapshot())];
            },
            RefreshAllAsync: async () => {
                proxyCalls.push('refreshAll');
                return [JSON.stringify(makeSnapshot({providers: [makeProvider()]}))];
            },
        });
        await flushPromises();

        // refresh + openSettings 都在待办队列 → 两个都回放，不补 fetch
        assert.ok(proxyCalls.includes('refreshAll'));
        assert.ok(proxyCalls.includes('openSettings'));
        assert.ok(!proxyCalls.includes('getAllQuotas'));
        // refresh 产生快照；openSettings 不产生快照
        assert.equal(snapshots.length, 1);
    });

    it('reports offline when manual refresh activation is throttled', async () => {
        const errors = [];
        const client = new QuotaClient({
            onError: (logMessage, uiMessage) => errors.push({logMessage, uiMessage}),
        });
        client._requestDaemonActivation = () => false;

        assert.equal(await client.refreshAll(), false);

        assert.deepEqual(errors, [{
            logMessage: 'D-Bus activation request skipped for manual refresh',
            uiMessage: 'BananaTray daemon not running',
        }]);
    });
});
