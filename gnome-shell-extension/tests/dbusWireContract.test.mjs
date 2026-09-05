// Rust producer 与 GNOME validator/presentation 共用的 schema v1 wire golden。

import {describe, it} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';

import {SUPPORTED_SCHEMA_VERSION, validateSnapshot} from '../quotaClient.js';
import {normalizeStatusKind} from '../quotaPresentation.js';

const fixture = JSON.parse(readFileSync(
    new URL('./fixtures/dbus-v1-wire.json', import.meta.url),
    'utf8',
));

function snapshot(statusKind) {
    return {
        schema_version: fixture.schema_version,
        header: {
            status_text: statusKind,
            status_kind: statusKind,
        },
        providers: [],
    };
}

describe('D-Bus schema v1 status_kind wire contract', () => {
    it('accepts and presents every golden wire value without warnings', () => {
        assert.equal(SUPPORTED_SCHEMA_VERSION, fixture.schema_version);

        for (const statusKind of fixture.header_status_kinds) {
            const warnings = [];
            validateSnapshot(snapshot(statusKind), warning => warnings.push(warning));

            assert.deepEqual(warnings, [], `${statusKind} must be a known validator value`);
            assert.equal(normalizeStatusKind(statusKind), statusKind.toLowerCase());
        }
    });

    it('preserves forward compatibility but warns on an unknown wire value', () => {
        const warnings = [];
        assert.doesNotThrow(() => validateSnapshot(snapshot('FutureState'), warning => warnings.push(warning)));
        assert.match(warnings.join('\n'), /header\.status_kind: FutureState/);
        assert.equal(normalizeStatusKind('FutureState'), 'stale');
    });
});
