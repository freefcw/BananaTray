import {readFileSync} from 'node:fs';

const paths = {
    client: process.env.BANANATRAY_DBUS_CONTRACT_CLIENT ?? 'gnome-shell-extension/quotaClient.js',
    jsContract: process.env.BANANATRAY_DBUS_CONTRACT_JS ?? 'gnome-shell-extension/dbusContract.js',
    fixture: process.env.BANANATRAY_DBUS_CONTRACT_FIXTURE ?? 'gnome-shell-extension/tests/fixtures/dbus-v1-wire.json',
    mock: process.env.BANANATRAY_DBUS_CONTRACT_MOCK ?? 'scripts/gnome-extension-mock-daemon.js',
    rustIface: process.env.BANANATRAY_DBUS_CONTRACT_RUST_IFACE ?? 'src/dbus/iface.rs',
    rustDto: process.env.BANANATRAY_DBUS_CONTRACT_RUST_DTO ?? 'src/application/selectors/dbus_dto.rs',
};

const EXPECTED_BUS_ID = 'com.bananatray.Daemon';
const EXPECTED_DBUS_PATH = '/com/bananatray/Daemon';
const EXPECTED_SCHEMA_VERSION = 1;

function read(path) {
    return readFileSync(path, 'utf8');
}

function fail(message) {
    console.error(`error: ${message}`);
    process.exitCode = 1;
}

function extractStringConst(source, name, path) {
    const match = source.match(new RegExp(`(?:export\\s+)?const\\s+${name}\\s*=\\s*['"]([^'"]+)['"]\\s*;`));
    if (!match) {
        fail(`${path} must define string const ${name}`);
        return null;
    }
    return match[1];
}

function extractNumberConst(source, name, path) {
    const match = source.match(new RegExp(`(?:export\\s+)?const\\s+${name}\\s*=\\s*(\\d+)\\s*;`));
    if (!match) {
        fail(`${path} must define number const ${name}`);
        return null;
    }
    return Number.parseInt(match[1], 10);
}

function extractRustSchemaVersion(source, path) {
    const match = source.match(/pub\s+const\s+DBUS_QUOTA_SCHEMA_VERSION:\s*u32\s*=\s*(\d+)\s*;/);
    if (!match) {
        fail(`${path} must define DBUS_QUOTA_SCHEMA_VERSION`);
        return null;
    }
    return Number.parseInt(match[1], 10);
}

function extractQuotedValues(source) {
    return [...source.matchAll(/['"]([^'"]+)['"]/g)].map(match => match[1]);
}

function extractJsFrozenStringArray(source, name, path) {
    const match = source.match(new RegExp(`export\\s+const\\s+${name}\\s*=\\s*Object\\.freeze\\(\\[([\\s\\S]*?)\\]\\)\\s*;`));
    if (!match) {
        fail(`${path} must define frozen string array ${name}`);
        return [];
    }
    return extractQuotedValues(match[1]);
}

function extractRustStringArray(source, name, path) {
    const match = source.match(new RegExp(`pub\\s+const\\s+${name}\\s*:\\s*\\[&str;\\s*\\d+\\]\\s*=\\s*\\[([\\s\\S]*?)\\]\\s*;`));
    if (!match) {
        fail(`${path} must define string array ${name}`);
        return [];
    }
    return extractQuotedValues(match[1]);
}

function extractInterfaceXml(source, path) {
    const match = source.match(/const\s+DBUS_INTERFACE_XML\s*=\s*`([\s\S]*?)`\s*;/);
    if (!match) {
        fail(`${path} must define DBUS_INTERFACE_XML`);
        return null;
    }
    return normalizeXml(match[1]);
}

function normalizeXml(xml) {
    return xml
        .trim()
        .split('\n')
        .map(line => line.trim())
        .filter(line => line.length > 0)
        .join('\n');
}

function assertEqual(actual, expected, label) {
    if (actual !== expected)
        fail(`${label} mismatch: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}

function assertMatches(source, regex, label) {
    if (!regex.test(source))
        fail(`missing D-Bus contract fragment: ${label}`);
}

function assertSharedValue(clientValue, mockValue, expectedValue, clientLabel, mockLabel, sharedLabel) {
    assertEqual(clientValue, expectedValue, clientLabel);
    assertEqual(mockValue, expectedValue, mockLabel);
    assertEqual(clientValue, mockValue, sharedLabel);
}

function assertStructFields(source, structName, fields) {
    const match = source.match(new RegExp(`pub\\s+struct\\s+${structName}\\s*\\{([\\s\\S]*?)\\n\\}`));
    if (!match) {
        fail(`Rust DTO must define ${structName}`);
        return;
    }

    for (const field of fields) {
        if (!new RegExp(`pub\\s+${field}\\s*:`).test(match[1]))
            fail(`${structName} must expose field ${field}`);
    }
}

const client = read(paths.client);
const jsContract = read(paths.jsContract);
const fixture = JSON.parse(read(paths.fixture));
const mock = read(paths.mock);
const rustIface = read(paths.rustIface);
const rustDto = read(paths.rustDto);

const clientBusId = extractStringConst(client, 'DBUS_ID', paths.client);
const mockBusId = extractStringConst(mock, 'DBUS_ID', paths.mock);
const clientPath = extractStringConst(client, 'DBUS_PATH', paths.client);
const mockPath = extractStringConst(mock, 'DBUS_PATH', paths.mock);
const clientSchemaVersion = extractNumberConst(client, 'SUPPORTED_SCHEMA_VERSION', paths.client);
const mockSchemaVersion = extractNumberConst(mock, 'SCHEMA_VERSION', paths.mock);
const rustSchemaVersion = extractRustSchemaVersion(rustDto, paths.rustDto);
const jsStatusKinds = extractJsFrozenStringArray(jsContract, 'STATUS_KIND_WIRE_VALUES', paths.jsContract);
const rustStatusKinds = extractRustStringArray(rustDto, 'DBUS_HEADER_STATUS_KIND_WIRE_VALUES', paths.rustDto);
const clientXml = extractInterfaceXml(client, paths.client);
const mockXml = extractInterfaceXml(mock, paths.mock);

assertSharedValue(
    clientBusId,
    mockBusId,
    EXPECTED_BUS_ID,
    `${paths.client} DBUS_ID`,
    `${paths.mock} DBUS_ID`,
    'GNOME Extension client/mock DBUS_ID',
);
assertSharedValue(
    clientPath,
    mockPath,
    EXPECTED_DBUS_PATH,
    `${paths.client} DBUS_PATH`,
    `${paths.mock} DBUS_PATH`,
    'GNOME Extension client/mock DBUS_PATH',
);
assertSharedValue(
    clientSchemaVersion,
    mockSchemaVersion,
    EXPECTED_SCHEMA_VERSION,
    `${paths.client} SUPPORTED_SCHEMA_VERSION`,
    `${paths.mock} SCHEMA_VERSION`,
    'GNOME Extension client/mock schema version',
);
assertEqual(rustSchemaVersion, EXPECTED_SCHEMA_VERSION, `${paths.rustDto} DBUS_QUOTA_SCHEMA_VERSION`);
assertEqual(clientSchemaVersion, rustSchemaVersion, 'GNOME Extension/Rust DTO schema version');
assertEqual(fixture.schema_version, EXPECTED_SCHEMA_VERSION, `${paths.fixture} schema_version`);
assertEqual(JSON.stringify(jsStatusKinds), JSON.stringify(fixture.header_status_kinds), 'GNOME Extension/fixture status_kind values');
assertEqual(JSON.stringify(rustStatusKinds), JSON.stringify(fixture.header_status_kinds), 'Rust DTO/fixture status_kind values');

assertMatches(client, /validateEnumField\(header\.status_kind,\s*STATUS_KIND_VALUES,\s*['"]header\.status_kind['"]/, 'header.status_kind validator');
assertMatches(rustDto, /status_kind:\s*format_header_status_kind\(status_kind\)\.to_string\(\)/, 'explicit Rust header.status_kind formatter');

assertEqual(clientXml, mockXml, 'GNOME Extension client/mock DBUS_INTERFACE_XML');

for (const fragment of [
    '<interface name="com.bananatray.Daemon">',
    '<method name="GetAllQuotas">',
    '<method name="RefreshAll">',
    '<method name="OpenSettings"/>',
    '<signal name="RefreshComplete">',
    '<property name="IsActive" type="b" access="read"/>',
]) {
    if (!clientXml.includes(fragment))
        fail(`DBUS_INTERFACE_XML must contain ${fragment}`);
}

for (const [label, regex] of [
    ['zbus interface name', /#\[zbus::interface\(name\s*=\s*"com\.bananatray\.Daemon"\)\]/],
    ['GetAllQuotas method', /fn\s+get_all_quotas\s*\(/],
    ['RefreshAll method', /fn\s+refresh_all\s*\(/],
    ['OpenSettings method', /fn\s+open_settings\s*\(/],
    ['RefreshComplete signal', /#\[zbus\(signal\)\][\s\S]*async\s+fn\s+refresh_complete\s*\(/],
    ['IsActive property', /#\[zbus\(property\)\][\s\S]*fn\s+is_active\s*\(/],
]) {
    assertMatches(rustIface, regex, label);
}

assertStructFields(rustDto, 'DBusQuotaSnapshot', ['schema_version', 'providers', 'header']);
assertStructFields(rustDto, 'DBusHeaderInfo', ['status_text', 'status_kind', 'elapsed_secs']);

// mock daemon 的 buildSnapshot 必须在 header payload 里实际发射 elapsed_secs，
// 否则 mock 与真实 producer 的契约漂移不会被 CI 捕获。
// 精确匹配 header: { ... } 块内容（到 header 闭合的 },），检查 elapsed_secs 在其中。
// 避免字段出现在注释 / provider body / 别处时误过。
const mockSnapshotBody = mock.match(/function\s+buildSnapshot\(\)\s*\{([\s\S]*?)\n\}/);
if (!mockSnapshotBody) {
    fail(`${paths.mock} must define buildSnapshot()`);
} else {
    // header 块：从 "header: {" 到行首 "}," 闭合（header 是 snapshot 的第一个字段）
    const headerBlock = mockSnapshotBody[1].match(/header:\s*\{([\s\S]*?)\n\s*\},/);
    if (!headerBlock) {
        fail(`${paths.mock} buildSnapshot must define a header object`);
    } else if (!/\belapsed_secs\s*:/.test(headerBlock[1])) {
        fail(`${paths.mock} buildSnapshot header must include elapsed_secs to match DBusHeaderInfo`);
    }
}
assertStructFields(rustDto, 'DBusProviderEntry', [
    'id',
    'display_name',
    'icon_asset',
    'connection',
    'account_email',
    'account_tier',
    'quotas',
    'worst_status',
]);
assertStructFields(rustDto, 'DBusQuotaEntry', [
    'label',
    'used',
    'limit',
    'status_level',
    'display_text',
    'bar_ratio',
    'quota_type_key',
]);

if (process.exitCode)
    process.exit();

console.log('GNOME D-Bus contract check passed');
