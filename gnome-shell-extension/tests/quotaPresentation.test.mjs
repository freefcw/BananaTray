// quotaPresentation.js 纯函数单元测试
//
// 运行: node --import ./gnome-shell-extension/tests/register.mjs \
//            --test ./gnome-shell-extension/tests/quotaPresentation.test.mjs

import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
    normalizeStatusLevel,
    normalizeConnection,
    normalizeStatusKind,
    headerStatusText,
    providerVisualLevel,
    statusBadgeLabel,
    connectionLabel,
    quotaRatio,
    sortedQuotas,
    providerInitials,
    summarizeProviders,
} from '../quotaPresentation.js';

// -- Helpers --

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

function makeQuota(overrides = {}) {
    return {
        label: 'Requests',
        used: 50,
        limit: 100,
        status_level: 'Green',
        display_text: '50%',
        quota_type_key: 'requests',
        ...overrides,
    };
}

// ============================================================
// normalizeStatusLevel
// ============================================================
describe('normalizeStatusLevel', () => {
    it('normalizes known values (case-insensitive)', () => {
        assert.equal(normalizeStatusLevel('green'), 'green');
        assert.equal(normalizeStatusLevel('Green'), 'green');
        assert.equal(normalizeStatusLevel('GREEN'), 'green');
        assert.equal(normalizeStatusLevel('Yellow'), 'yellow');
        assert.equal(normalizeStatusLevel('Red'), 'red');
    });

    it('defaults unknown values to yellow', () => {
        assert.equal(normalizeStatusLevel('purple'), 'yellow');
        assert.equal(normalizeStatusLevel(''), 'yellow');
        assert.equal(normalizeStatusLevel(null), 'yellow');
        assert.equal(normalizeStatusLevel(undefined), 'yellow');
    });
});

// ============================================================
// normalizeConnection
// ============================================================
describe('normalizeConnection', () => {
    it('normalizes known values (case-insensitive)', () => {
        assert.equal(normalizeConnection('connected'), 'connected');
        assert.equal(normalizeConnection('Connected'), 'connected');
        assert.equal(normalizeConnection('REFRESHING'), 'refreshing');
        assert.equal(normalizeConnection('Error'), 'error');
        assert.equal(normalizeConnection('Disconnected'), 'disconnected');
    });

    it('defaults unknown values to disconnected', () => {
        assert.equal(normalizeConnection('offline'), 'disconnected');
        assert.equal(normalizeConnection(''), 'disconnected');
        assert.equal(normalizeConnection(null), 'disconnected');
        assert.equal(normalizeConnection(undefined), 'disconnected');
    });
});

// ============================================================
// normalizeStatusKind
// ============================================================
describe('normalizeStatusKind', () => {
    it('normalizes known values (case-insensitive)', () => {
        assert.equal(normalizeStatusKind('synced'), 'synced');
        assert.equal(normalizeStatusKind('Synced'), 'synced');
        assert.equal(normalizeStatusKind('SYNCING'), 'syncing');
        assert.equal(normalizeStatusKind('Stale'), 'stale');
        assert.equal(normalizeStatusKind('Offline'), 'offline');
    });

    it('defaults unknown values to stale', () => {
        assert.equal(normalizeStatusKind('unknown'), 'stale');
        assert.equal(normalizeStatusKind(''), 'stale');
        assert.equal(normalizeStatusKind(null), 'stale');
        assert.equal(normalizeStatusKind(undefined), 'stale');
    });
});

// ============================================================
// headerStatusText
// ============================================================
describe('headerStatusText', () => {
    it('localizes stable status kinds', () => {
        assert.equal(headerStatusText('Synced'), 'Synced');
        assert.equal(headerStatusText('Syncing'), 'Syncing');
        assert.equal(headerStatusText('Offline'), 'Offline');
    });

    it('formats stale elapsed time in minutes and hours', () => {
        assert.equal(headerStatusText('Stale', 59), '0 minutes ago');
        assert.equal(headerStatusText('Stale', 600), '10 minutes ago');
        assert.equal(headerStatusText('Stale', 7200), '2 hours ago');
    });

    it('returns null for stale without elapsed time', () => {
        assert.equal(headerStatusText('Stale', undefined), null);
    });
});

// ============================================================
// providerVisualLevel
// ============================================================
describe('providerVisualLevel', () => {
    it('returns red for error + no quotas', () => {
        const provider = makeProvider({connection: 'Error', quotas: []});
        assert.equal(providerVisualLevel(provider), 'red');
    });

    it('returns worst_status for error + cached quotas', () => {
        const provider = makeProvider({
            connection: 'Error',
            worst_status: 'Yellow',
            quotas: [makeQuota()],
        });
        assert.equal(providerVisualLevel(provider), 'yellow');
    });

    it('returns yellow for refreshing', () => {
        const provider = makeProvider({connection: 'Refreshing', worst_status: 'Green'});
        assert.equal(providerVisualLevel(provider), 'yellow');
    });

    it('returns yellow for disconnected', () => {
        const provider = makeProvider({connection: 'Disconnected', worst_status: 'Green'});
        assert.equal(providerVisualLevel(provider), 'yellow');
    });

    it('returns worst_status for connected', () => {
        assert.equal(providerVisualLevel(makeProvider({worst_status: 'Green'})), 'green');
        assert.equal(providerVisualLevel(makeProvider({worst_status: 'Red'})), 'red');
    });
});

// ============================================================
// statusBadgeLabel
// ============================================================
describe('statusBadgeLabel', () => {
    it('returns correct badge labels', () => {
        assert.equal(statusBadgeLabel('red'), 'OUT');
        assert.equal(statusBadgeLabel('yellow'), 'LOW');
        assert.equal(statusBadgeLabel('green'), 'OK');
    });

    it('defaults to OK for unknown level', () => {
        assert.equal(statusBadgeLabel('purple'), 'OK');
    });
});

// ============================================================
// connectionLabel
// ============================================================
describe('connectionLabel', () => {
    it('returns correct labels', () => {
        assert.equal(connectionLabel('Connected'), 'Connected');
        assert.equal(connectionLabel('Refreshing'), 'Refreshing');
        assert.equal(connectionLabel('Error'), 'Error');
        assert.equal(connectionLabel('Disconnected'), 'Disconnected');
    });

    it('defaults unknown to Disconnected', () => {
        assert.equal(connectionLabel('offline'), 'Disconnected');
        assert.equal(connectionLabel(null), 'Disconnected');
    });
});

// ============================================================
// quotaRatio
// ============================================================
describe('quotaRatio', () => {
    it('uses bar_ratio when available', () => {
        assert.equal(quotaRatio({bar_ratio: 0.75, used: 10, limit: 100}), 0.75);
    });

    it('clamps bar_ratio to [0, 1]', () => {
        assert.equal(quotaRatio({bar_ratio: 1.5, used: 10, limit: 100}), 1);
        assert.equal(quotaRatio({bar_ratio: -0.5, used: 10, limit: 100}), 0);
    });

    it('falls back to used/limit when bar_ratio is absent', () => {
        assert.equal(quotaRatio({used: 25, limit: 100}), 0.25);
        assert.equal(quotaRatio({used: 100, limit: 100}), 1);
    });

    it('returns 0 when limit is 0 or negative', () => {
        assert.equal(quotaRatio({used: 50, limit: 0}), 0);
        assert.equal(quotaRatio({used: 50, limit: -10}), 0);
    });

    it('clamps used/limit to [0, 1]', () => {
        assert.equal(quotaRatio({used: 200, limit: 100}), 1);
        assert.equal(quotaRatio({used: -10, limit: 100}), 0);
    });

    it('ignores non-finite bar_ratio', () => {
        assert.equal(quotaRatio({bar_ratio: NaN, used: 50, limit: 100}), 0.5);
        assert.equal(quotaRatio({bar_ratio: Infinity, used: 50, limit: 100}), 0.5);
    });
});

// ============================================================
// sortedQuotas
// ============================================================
describe('sortedQuotas', () => {
    it('sorts by status severity descending', () => {
        const provider = makeProvider({
            quotas: [
                makeQuota({label: 'A', status_level: 'Green', used: 10, limit: 100}),
                makeQuota({label: 'B', status_level: 'Red', used: 90, limit: 100}),
                makeQuota({label: 'C', status_level: 'Yellow', used: 50, limit: 100}),
            ],
        });
        const sorted = sortedQuotas(provider);
        assert.deepEqual(sorted.map(q => q.label), ['B', 'C', 'A']);
    });

    it('breaks ties by ratio ascending (lower ratio = more remaining = better)', () => {
        const provider = makeProvider({
            quotas: [
                makeQuota({label: 'high', status_level: 'Green', used: 80, limit: 100}),
                makeQuota({label: 'low', status_level: 'Green', used: 20, limit: 100}),
            ],
        });
        const sorted = sortedQuotas(provider);
        assert.deepEqual(sorted.map(q => q.label), ['low', 'high']);
    });

    it('does not mutate original', () => {
        const quotas = [
            makeQuota({label: 'A', status_level: 'Red'}),
            makeQuota({label: 'B', status_level: 'Green'}),
        ];
        const provider = makeProvider({quotas});
        sortedQuotas(provider);
        assert.equal(quotas[0].label, 'A'); // unchanged
    });

    it('handles empty/missing quotas', () => {
        assert.deepEqual(sortedQuotas(makeProvider({quotas: []})), []);
        assert.deepEqual(sortedQuotas(makeProvider({quotas: undefined})), []);
    });
});

// ============================================================
// providerInitials
// ============================================================
describe('providerInitials', () => {
    it('takes first letter of first two words', () => {
        assert.equal(providerInitials({display_name: 'Claude Pro'}), 'CP');
        assert.equal(providerInitials({display_name: 'GitHub Copilot Plus'}), 'GC');
    });

    it('takes first 2 chars for single-word name', () => {
        assert.equal(providerInitials({display_name: 'Cursor'}), 'CU');
        assert.equal(providerInitials({display_name: 'Amp'}), 'AM');
    });

    it('falls back to id', () => {
        assert.equal(providerInitials({id: 'codex'}), 'CO');
    });

    it('handles empty/missing', () => {
        // '?' has only 1 char, slice(0, 2) = '?'
        assert.equal(providerInitials({}), '?');
    });

    it('uppercases result', () => {
        assert.equal(providerInitials({display_name: 'cursor pro'}), 'CP');
    });
});

// ============================================================
// summarizeProviders
// ============================================================
describe('summarizeProviders', () => {
    it('returns defaults for empty list', () => {
        const summary = summarizeProviders([]);
        assert.equal(summary.total, 0);
        assert.equal(summary.connected, 0);
        assert.equal(summary.attention, 0);
        assert.equal(summary.panelLevel, 'green');
    });

    it('counts connections correctly', () => {
        const providers = [
            makeProvider({connection: 'Connected', worst_status: 'Green'}),
            makeProvider({connection: 'Connected', worst_status: 'Green'}),
            makeProvider({connection: 'Error', quotas: []}),
            makeProvider({connection: 'Refreshing'}),
            makeProvider({connection: 'Disconnected'}),
        ];
        const summary = summarizeProviders(providers);
        assert.equal(summary.total, 5);
        assert.equal(summary.connected, 2);
        assert.equal(summary.error, 1);
        assert.equal(summary.refreshing, 1);
        assert.equal(summary.disconnected, 1);
    });

    it('computes attention count', () => {
        const providers = [
            makeProvider({connection: 'Connected', worst_status: 'Green'}),  // no attention
            makeProvider({connection: 'Connected', worst_status: 'Yellow'}), // attention (not green)
            makeProvider({connection: 'Error', quotas: []}),                 // attention (not connected)
        ];
        const summary = summarizeProviders(providers);
        assert.equal(summary.attention, 2);
    });

    it('generates panel text from first provider when all-green', () => {
        const providers = [
            makeProvider({
                display_name: 'Claude',
                connection: 'Connected',
                worst_status: 'Green',
                quotas: [makeQuota({display_text: '60 left'})],
            }),
            makeProvider({connection: 'Connected', worst_status: 'Green'}),
        ];
        const summary = summarizeProviders(providers);
        assert.equal(summary.panelText, 'Claude 60 left');
    });

    it('keeps panel state on first provider even when another provider is worse', () => {
        const providers = [
            makeProvider({
                display_name: 'Codex',
                connection: 'Connected',
                worst_status: 'Green',
                quotas: [makeQuota({display_text: '80 left', status_level: 'Green'})],
            }),
            makeProvider({
                display_name: 'Claude',
                connection: 'Connected',
                worst_status: 'Red',
                quotas: [makeQuota({display_text: '95% used', status_level: 'Red'})],
            }),
        ];
        const summary = summarizeProviders(providers);
        assert.equal(summary.panelLevel, 'green');
        assert.equal(summary.panelText, 'Codex 80 left');
    });

    it('generates panel text with connection label when first provider has no quotas', () => {
        const providers = [
            makeProvider({
                display_name: 'Gemini',
                connection: 'Error',
                worst_status: 'Red',
                quotas: [],
            }),
            makeProvider({connection: 'Connected', worst_status: 'Green'}),
        ];
        const summary = summarizeProviders(providers);
        assert.equal(summary.panelLevel, 'red');
        assert.equal(summary.panelText, 'Gemini Error');
    });

    it('includes header text with ngettext counts', () => {
        const providers = [
            makeProvider({connection: 'Connected', worst_status: 'Green'}),
        ];
        const summary = summarizeProviders(providers);
        // Mock ngettext returns singular for count=1: "%d provider", "%d connected"
        assert.ok(summary.headerText.includes('1'));
    });

    it('includes error/offline/refreshing in header when present', () => {
        const providers = [
            makeProvider({connection: 'Error', quotas: []}),
            makeProvider({connection: 'Disconnected'}),
            makeProvider({connection: 'Refreshing'}),
        ];
        const summary = summarizeProviders(providers);
        assert.ok(summary.headerText.includes('error'));
        assert.ok(summary.headerText.includes('offline'));
        assert.ok(summary.headerText.includes('refreshing'));
    });
});
