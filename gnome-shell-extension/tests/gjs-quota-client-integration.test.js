// quotaClient.js GJS 真实 D-Bus 集成测试
//
// 运行环境：dbus-run-session + gjs --module
// 运行入口：scripts/test-gnome-extension-gjs.sh
//
// 本文件在同一个 GJS 进程内：
//   1. startMockDaemon() 在 session 总线上注册 mock daemon（见 ./gjs-mock-daemon.js）
//   2. import 真实的 quotaClient.js，创建 QuotaClient 实例
//   3. 通过 onReady / onSnapshot / onVanished / onError 回调断言端到端 D-Bus 行为
//   4. imports.system.exit(0/1) 传播通过/失败给 CI
//
// 与 Node mock 单测（quotaClient.test.mjs）的区别：
//   - Node 单测用 mock-gi.mjs stub 掉 Gio/GLib，不涉及真实 D-Bus
//   - 本测试用真实 gi://Gio + dbus-run-session，验证 Gio.DBusProxy / 信号订阅 /
//     schema 校验在真实 GJS 引擎 + 真实 D-Bus 总线上的行为
//
// 每个测试用例自行 start/stop 独立 mock daemon（try/finally），不共享全局
// 可变状态，保证用例间隔离。

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {QuotaClient, SUPPORTED_SCHEMA_VERSION} from '../quotaClient.js';
import {
    startMockDaemon,
    buildSnapshot,
    buildMalformedSnapshot,
} from './gjs-mock-daemon.js';

// ─── 测试基础设施 ───────────────────────────────────────────────────────────

let _passed = 0;
let _failed = 0;

function assert(condition, message) {
    if (condition) {
        _passed += 1;
        print(`  ✓ ${message}`);
    } else {
        _failed += 1;
        print(`  ✗ ${message}`);
    }
}

function assertEqual(actual, expected, message) {
    const equal = JSON.stringify(actual) === JSON.stringify(expected);
    assert(equal, `${message} (expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)})`);
}

// 等待条件成立或超时。GJS --module 没有自动 MainLoop，靠 timeout 源驱动。
function waitForCondition(predicate, timeoutMs = 5000, intervalMs = 50) {
    return new Promise((resolve, reject) => {
        const deadline = GLib.get_monotonic_time() + timeoutMs * 1000;
        const id = GLib.timeout_add(GLib.PRIORITY_DEFAULT, intervalMs, () => {
            if (predicate()) {
                GLib.source_remove(id);
                resolve();
                return GLib.SOURCE_REMOVE;
            }
            if (GLib.get_monotonic_time() > deadline) {
                GLib.source_remove(id);
                reject(new Error(`waitForCondition timed out after ${timeoutMs}ms`));
                return GLib.SOURCE_REMOVE;
            }
            return GLib.SOURCE_CONTINUE;
        });
    });
}

function sleep(ms) {
    return new Promise((resolve) => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => {
            resolve();
            return GLib.SOURCE_REMOVE;
        });
    });
}

// QuotaClient.start() 会调 StartServiceByName 请求 D-Bus activation。测试环境没有
// 安装 .service activation 文件，因此 activation 必然失败（ServiceUnknown）。这是
// 预期行为——mock daemon 已直接注册在总线，bus_watch_name 会发现它。非预期错误
// 才打印。
function suppressActivationError(log) {
    if (log.includes('StartServiceByName') || log.includes('ServiceUnknown'))
        return;
    print(`  ERROR: ${log}`);
}

// ─── 测试用例 ───────────────────────────────────────────────────────────────

async function testProxyReadyAndFetchQuotas() {
    print('\n[Test 1] proxy ready → fetchQuotas → onSnapshot');

    const daemon = await startMockDaemon();
    try {
        let onReadyFired = false;
        let receivedSnapshot = null;

        const client = new QuotaClient({
            onReady: () => { onReadyFired = true; },
            onSnapshot: (snapshot) => { receivedSnapshot = snapshot; },
            onError: suppressActivationError,
        });

        client.start();
        try {
            // bus_watch_name 是异步的，等 daemon 出现 + proxy ready
            await waitForCondition(() => onReadyFired, 5000);
            assert(onReadyFired, 'onReady 回调在 daemon 出现后触发');

            // _onProxyReady 会自动调一次 fetchQuotas，等快照到达
            await waitForCondition(() => receivedSnapshot !== null, 5000);
            assert(receivedSnapshot !== null, 'fetchQuotas 通过 GetAllQuotasAsync 获取到快照');
            assertEqual(receivedSnapshot.schema_version, SUPPORTED_SCHEMA_VERSION, '快照 schema_version 正确');
            assertEqual(receivedSnapshot.providers[0].id, 'claude', '快照包含 claude provider');
        } finally {
            client.destroy();
        }
    } finally {
        daemon.stop();
    }
}

async function testRefreshAllAndRefreshCompleteSignal() {
    print('\n[Test 2] refreshAll → RefreshAllAsync → RefreshComplete 信号 → onSnapshot');

    const daemon = await startMockDaemon();
    try {
        let snapshotCount = 0;
        let lastSnapshot = null;

        const client = new QuotaClient({
            onSnapshot: (snapshot) => { snapshotCount += 1; lastSnapshot = snapshot; },
            onError: suppressActivationError,
        });

        client.start();
        try {
            await waitForCondition(() => snapshotCount >= 1, 5000);
            assert(snapshotCount >= 1, '初始 fetch 获取到快照');

            // 手动刷新：RefreshAll 使 daemon 递增 refreshCount 并 emit RefreshComplete。
            const result = await client.refreshAll();
            assert(result === true, 'refreshAll 在 proxy ready 时返回 true');
            // refreshAll 失败（如契约漂移）时，RefreshComplete 推送的验证没有意义，
            // 也不再产生 refresh 后的状态，直接停止本用例以避免误导。
            if (!result)
                return;

            // RefreshComplete 应推送 refreshCount 递增后的新快照。
            // 用确定的目标状态（refresh 后的 status_text）而非前后比对，
            // 避免 signal 回调与 statusBefore 取值的竞态。
            const targetText = `Mock refresh #${daemon.service.refreshCount}`;
            await waitForCondition(
                () => lastSnapshot?.header.status_text === targetText,
                5000,
            );
            assert(
                lastSnapshot.header.status_text === targetText,
                `RefreshComplete 推送刷新后的快照 (${targetText})`,
            );
        } finally {
            client.destroy();
        }
    } finally {
        daemon.stop();
    }
}

async function testDaemonVanishedAndReconnect() {
    print('\n[Test 3] daemon 消失 → onVanished → daemon 回归 → onReady');

    let vanishedFired = false;
    let readyCount = 0;
    let client = null;
    let daemon1 = null;
    let daemon2 = null;

    try {
        daemon1 = await startMockDaemon();
        client = new QuotaClient({
            onReady: () => { readyCount += 1; },
            onVanished: () => { vanishedFired = true; },
            onSnapshot: () => {},
            onError: suppressActivationError,
        });

        client.start();
        await waitForCondition(() => readyCount === 1, 5000);
        assert(readyCount === 1, '初次连接 onReady 触发 (readyCount=1)');

        // 停掉第一个 daemon，等 onVanished
        daemon1.stop();
        daemon1 = null;
        await waitForCondition(() => vanishedFired, 5000);
        assert(vanishedFired, 'daemon 消失后 onVanished 触发');

        // 重新启动 daemon，等 onReady 再次触发
        await sleep(200); // 让 bus_watch_name 稳定
        daemon2 = await startMockDaemon();
        await waitForCondition(() => readyCount === 2, 5000);
        assert(readyCount === 2, 'daemon 回归后 onReady 再次触发 (readyCount=2)');
    } finally {
        client?.destroy();
        daemon1?.stop();
        daemon2?.stop();
    }
}

async function testSchemaValidationOnMalformedSnapshot() {
    print('\n[Test 4] malformed snapshot → validateSnapshot 抛出 → onSnapshot 不触发');

    // 注入畸形快照生成器，无需对原型 monkey-patch。
    const daemon = await startMockDaemon({getSnapshot: buildMalformedSnapshot});
    try {
        let errorFired = false;
        let snapshotFired = false;

        const client = new QuotaClient({
            onSnapshot: () => { snapshotFired = true; },
            onError: () => { errorFired = true; },
        });

        client.start();
        try {
            // proxy ready 后自动 fetchQuotas 会触发 validateSnapshot 失败
            await waitForCondition(() => errorFired, 5000);
            assert(errorFired, 'schema_version 不匹配时 onError 触发');
            assert(!snapshotFired, '校验失败时 onSnapshot 不触发');
        } finally {
            client.destroy();
        }
    } finally {
        daemon.stop();
    }
}

async function testOpenSettings() {
    print('\n[Test 5] openSettings → OpenSettingsAsync → mock daemon 收到调用');

    const daemon = await startMockDaemon();
    try {
        let onReadyFired = false;
        const client = new QuotaClient({
            onReady: () => { onReadyFired = true; },
            onSnapshot: () => {},
            onError: suppressActivationError,
        });

        client.start();
        try {
            await waitForCondition(() => onReadyFired, 5000);
            assert(onReadyFired, 'proxy ready 后 openSettings 可调用');

            const result = await client.openSettings();
            assert(result === true, 'openSettings 返回 true');
            assert(
                daemon.service.openSettingsCallCount === 1,
                `mock daemon 收到 OpenSettings 调用 (count=${daemon.service.openSettingsCallCount})`,
            );
        } finally {
            client.destroy();
        }
    } finally {
        daemon.stop();
    }
}

async function testDestroyStopsCallbacks() {
    print('\n[Test 6] destroy() 后不再收到 onSnapshot');

    const daemon = await startMockDaemon();
    try {
        let snapshotCount = 0;
        const client = new QuotaClient({
            onSnapshot: () => { snapshotCount += 1; },
            onError: suppressActivationError,
        });

        client.start();
        try {
            // 等初始 fetch 完成
            await waitForCondition(() => snapshotCount >= 1, 5000);
            assert(snapshotCount >= 1, '初始 fetch 触发 onSnapshot');

            client.destroy();
            const countAfterDestroy = snapshotCount;

            // destroy 后手动触发一次刷新，不应再产生 onSnapshot
            daemon.service.RefreshAll();
            await sleep(500);

            assert(
                snapshotCount === countAfterDestroy,
                `destroy() 后 onSnapshot 不再触发 (count 保持 ${countAfterDestroy})`,
            );
        } finally {
            client.destroy();
        }
    } finally {
        daemon.stop();
    }
}

// ─── 测试编排 ───────────────────────────────────────────────────────────────
// 每个测试用例自己管理 client/daemon 生命周期。若某用例失败（_failed > 0），
// 跳过后续用例以快速失败。

const TESTS = [
    ['proxy ready → fetch → onSnapshot', testProxyReadyAndFetchQuotas],
    ['refreshAll → RefreshComplete 信号', testRefreshAllAndRefreshCompleteSignal],
    ['daemon 消失 → onVanished → 回归', testDaemonVanishedAndReconnect],
    ['malformed snapshot → onError', testSchemaValidationOnMalformedSnapshot],
    ['openSettings 端到端', testOpenSettings],
    ['destroy 后无回调', testDestroyStopsCallbacks],
];


// ─── 主流程 ─────────────────────────────────────────────────────────────────
//
// GJS --module 模式不会自动起 GLib MainLoop；top-level await 的 continuation
// 需要事件循环驱动。这里显式创建 MainLoop，在主流程完成后 quit。

const _loop = GLib.MainLoop.new(null, false);

function finish(exitCode, summary) {
    print(summary);
    _loop.quit();
    imports.system.exit(exitCode);
}

async function main() {
    print('=== BananaTray GNOME Shell Extension GJS 集成测试 ===');
    print(`GJS ${imports.system.version ?? 'unknown'}, dbus-run-session session bus`);

    for (const [name, testFn] of TESTS) {
        if (_failed > 0) {
            print(`\n[S] 跳过「${name}」（已有用例失败，快速失败）`);
            continue;
        }
        await testFn();
    }

    const summary = `\n=== 结果: ${_passed} passed, ${_failed} failed ===`;
    finish(_failed === 0 ? 0 : 1, summary);
}

// 安全超时：防止任何 waitForCondition 死锁导致 CI 挂起
GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 30, () => {
    print('FATAL: 全局超时 (30s)，测试挂起');
    finish(1, '');
    return GLib.SOURCE_REMOVE;
});

// 启动 main，异常时打印并退出 1。MainLoop 驱动 async continuation。
main().catch((e) => {
    print(`FATAL: ${e.message}\n${e.stack ?? ''}`);
    finish(1, '');
});

_loop.run();
