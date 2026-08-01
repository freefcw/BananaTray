// GJS 集成测试专用 mock D-Bus daemon（ESM 模块）。
//
// 供 gnome-shell-extension/tests/gjs-quota-client-integration.test.js 复用，
// 在同一 GJS 进程内于 dbus-run-session 会话总线上注册 com.bananatray.Daemon，
// 用真实 Gio.DBusExportedObject 提供方法与信号。
//
// 常量（DBUS_ID / DBUS_PATH / SUPPORTED_SCHEMA_VERSION）直接 import 自
// quotaClient.js，避免与真实 client 的多份拷贝漂移。DBUS_INTERFACE_XML 无法
// 从 quotaClient.js import（未导出），因此在此单独定义；其与
// quotaClient.js 的 DBUS_INTERFACE_XML 的一致性由本集成测试自身捕获——若
// 方法/信号签名不匹配，proxy 调用（GetAllQuotas/RefreshAll/OpenSettings）或
// RefreshComplete 信号接收会失败。（scripts/check-gnome-dbus-contract.mjs 只
// 静态校验 production 的 quotaClient.js 与 gnome-extension-mock-daemon.js。）

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {
    DBUS_ID,
    DBUS_PATH,
    SUPPORTED_SCHEMA_VERSION,
} from '../quotaClient.js';

// 必须与 quotaClient.js 的 DBUS_INTERFACE_XML 保持一致。若这里的方法/信号/属性
// 与 client 不匹配，集成测试的 proxy 调用或信号接收会失败（本测试自行捕获），
// 不需要额外的静态校验。
export const MOCK_DAEMON_XML = `
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

// 生成一份符合 DBusQuotaSnapshot schema 的标准快照。
// refreshCount 用于让每次 RefreshAll 产生可区分的 status_text。
export function buildSnapshot(refreshCount) {
    const used = Math.min(100, 42 + refreshCount * 3);
    return {
        schema_version: SUPPORTED_SCHEMA_VERSION,
        header: {
            status_text: `Mock refresh #${refreshCount}`,
            status_kind: 'Synced',
            elapsed_secs: null,
        },
        providers: [
            {
                id: 'claude',
                display_name: 'Claude',
                icon_asset: 'src/icons/provider-claude.svg',
                connection: 'Connected',
                account_email: 'claude@example.test',
                account_tier: 'Pro',
                worst_status: 'Green',
                quotas: [
                    {
                        label: 'Session',
                        used,
                        limit: 100,
                        status_level: 'Green',
                        display_text: `${used}%`,
                        quota_type_key: 'session',
                        bar_ratio: used / 100,
                    },
                ],
            },
        ],
    };
}

// 生成一份 schema_version 不匹配的畸形快照，用于验证 client 的校验拒绝路径。
export function buildMalformedSnapshot() {
    return {schema_version: 999, header: {}, providers: []};
}

// D-Bus 服务实现。允许通过 options.getSnapshot 注入自定义快照生成逻辑，
// 使测试无需对原型做 monkey-patch 即可模拟病态/边缘响应。
class MockDaemonService {
    constructor({getSnapshot = buildSnapshot} = {}) {
        this.refreshCount = 0;
        this.openSettingsCallCount = 0;
        this._getSnapshot = (count) => getSnapshot(count);
        this._exportedObject = null;
    }

    get IsActive() {
        return true;
    }

    GetAllQuotas() {
        return JSON.stringify(this._getSnapshot(this.refreshCount));
    }

    RefreshAll() {
        this.refreshCount += 1;
        const jsonData = JSON.stringify(this._getSnapshot(this.refreshCount));
        if (this._exportedObject)
            this._exportedObject.emit_signal(
                'RefreshComplete',
                GLib.Variant.new('(s)', [jsonData]),
            );
        return jsonData;
    }

    OpenSettings() {
        this.openSettingsCallCount += 1;
    }
}

// 在 session 总线上注册 mock daemon，返回独立 handle。
// 每个 handle 拥有自己的 MockDaemonService 实例，测试可独立启停，
// 不共享全局可变状态。handle.stop() 幂等，可安全地在 finally 重复调用。
export function startMockDaemon(options = {}) {
    return new Promise((resolve, reject) => {
        const service = new MockDaemonService(options);
        const handle = {service, ownerId: 0, exportedObject: null, stopped: false};

        handle.ownerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            DBUS_ID,
            Gio.BusNameOwnerFlags.NONE,
            (connection) => {
                handle.exportedObject = Gio.DBusExportedObject.wrapJSObject(
                    MOCK_DAEMON_XML,
                    service,
                );
                service._exportedObject = handle.exportedObject;
                handle.exportedObject.export(connection, DBUS_PATH);
                resolve(handle);
            },
            null,
            // name-lost：正常 stop() 里 unown 会触发此回调，用 stopped 标记区分。
            () => {
                if (!handle.stopped)
                    reject(new Error('mock daemon lost bus name'));
            },
        );

        handle.stop = function stop() {
            this.stopped = true;
            if (this.exportedObject) {
                this.exportedObject.unexport();
                this.exportedObject = null;
            }
            if (this.ownerId) {
                Gio.bus_unown_name(this.ownerId);
                this.ownerId = 0;
            }
        };
    });
}
