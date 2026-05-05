import Gio from 'gi://Gio';

import {_} from './i18n.js';

export const DBUS_ID = 'com.bananatray.Daemon';
export const DBUS_PATH = '/com/bananatray/Daemon';
export const SUPPORTED_SCHEMA_VERSION = 1;

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

const QuotaProxy = Gio.DBusProxy.makeProxyWrapper(DBUS_INTERFACE_XML);

function isPlainObject(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isString(value) {
    return typeof value === 'string';
}

function isNumber(value) {
    return typeof value === 'number' && Number.isFinite(value);
}

function validateQuota(quota, providerId, index) {
    if (!isPlainObject(quota))
        throw new Error(`provider ${providerId} quota #${index} is not an object`);

    for (const field of ['label', 'status_level', 'display_text', 'quota_type_key']) {
        if (!isString(quota[field]))
            throw new Error(`provider ${providerId} quota #${index} missing string ${field}`);
    }

    for (const field of ['used', 'limit']) {
        if (!isNumber(quota[field]))
            throw new Error(`provider ${providerId} quota #${index} missing number ${field}`);
    }
}

function validateProvider(provider, index) {
    if (!isPlainObject(provider))
        throw new Error(`provider #${index} is not an object`);

    for (const field of ['id', 'display_name', 'icon_asset', 'connection', 'worst_status']) {
        if (!isString(provider[field]))
            throw new Error(`provider #${index} missing string ${field}`);
    }

    if (provider.account_email !== null && provider.account_email !== undefined && !isString(provider.account_email))
        throw new Error(`provider ${provider.id} account_email must be string or null`);
    if (provider.account_tier !== null && provider.account_tier !== undefined && !isString(provider.account_tier))
        throw new Error(`provider ${provider.id} account_tier must be string or null`);
    if (!Array.isArray(provider.quotas))
        throw new Error(`provider ${provider.id} quotas must be an array`);

    provider.quotas.forEach((quota, quotaIndex) => validateQuota(quota, provider.id, quotaIndex));
}

function validateSnapshot(data) {
    if (!isPlainObject(data))
        throw new Error('snapshot is not an object');
    if (data.schema_version !== SUPPORTED_SCHEMA_VERSION)
        throw new Error(`unsupported schema_version ${data.schema_version}`);
    if (!isPlainObject(data.header))
        throw new Error('snapshot header is not an object');
    if (!isString(data.header.status_text))
        throw new Error('snapshot header missing string status_text');
    if (!isString(data.header.status_kind))
        throw new Error('snapshot header missing string status_kind');
    if (!Array.isArray(data.providers))
        throw new Error('snapshot providers must be an array');

    data.providers.forEach((provider, providerIndex) => validateProvider(provider, providerIndex));
    return data;
}

function parseSnapshot(jsonData) {
    return validateSnapshot(JSON.parse(jsonData));
}

export class QuotaClient {
    constructor({onReady, onVanished, onSnapshot, onError}) {
        this._onReady = onReady;
        this._onVanished = onVanished;
        this._onSnapshot = onSnapshot;
        this._onError = onError;

        this._proxy = null;
        this._proxySignalId = 0;
        this._busWatchId = 0;
        this._proxyGeneration = 0;
        this._destroyed = false;
    }

    start() {
        this._busWatchId = Gio.bus_watch_name(
            Gio.BusType.SESSION,
            DBUS_ID,
            Gio.BusNameWatcherFlags.NONE,
            () => this._onDaemonAppeared(),
            () => this._onDaemonVanished(),
        );
    }

    async fetchQuotas() {
        const proxy = this._proxy;
        const generation = this._proxyGeneration;
        if (!proxy) return;

        try {
            const [jsonData] = await proxy.GetAllQuotasAsync();
            if (!this._isCurrentProxy(proxy, generation))
                return;
            this._emitSnapshot(parseSnapshot(jsonData));
        } catch (e) {
            if (!this._isCurrentProxy(proxy, generation))
                return;
            this._emitError(`GetAllQuotas failed: ${e.message}`, _('Failed to fetch quota data'));
        }
    }

    async refreshAll() {
        const proxy = this._proxy;
        const generation = this._proxyGeneration;
        if (!proxy) return;

        try {
            const [jsonData] = await proxy.RefreshAllAsync();
            if (!this._isCurrentProxy(proxy, generation))
                return;
            this._emitSnapshot(parseSnapshot(jsonData));
        } catch (e) {
            if (!this._isCurrentProxy(proxy, generation))
                return;
            this._emitError(`RefreshAll failed: ${e.message}`);
        }
    }

    async openSettings() {
        const proxy = this._proxy;
        const generation = this._proxyGeneration;
        if (!proxy) return;

        try {
            await proxy.OpenSettingsAsync();
        } catch (e) {
            if (!this._isCurrentProxy(proxy, generation))
                return;
            this._emitError(`OpenSettings failed: ${e.message}`);
        }
    }

    destroy() {
        this._destroyed = true;
        this._proxyGeneration++;
        if (this._busWatchId) {
            Gio.bus_unwatch_name(this._busWatchId);
            this._busWatchId = 0;
        }
        this._clearProxy();
    }

    _onDaemonAppeared() {
        this._emitLog('daemon appeared on D-Bus');
        const generation = ++this._proxyGeneration;

        try {
            new QuotaProxy(
                Gio.DBus.session,
                DBUS_ID,
                DBUS_PATH,
                (proxy, error) => {
                    if (this._destroyed || generation !== this._proxyGeneration)
                        return;

                    if (error !== null) {
                        this._emitError(`failed to create D-Bus proxy: ${error.message}`, _('Failed to connect to BananaTray daemon'));
                        return;
                    }

                    if (proxy === null) {
                        this._emitError('D-Bus proxy initialization returned null', _('Failed to connect to BananaTray daemon'));
                        return;
                    }

                    this._installProxy(proxy);
                    this._onReady?.();
                    this.fetchQuotas();
                },
                null,
                Gio.DBusProxyFlags.NONE,
            );
        } catch (e) {
            this._emitError(`failed to create D-Bus proxy: ${e.message}`, _('Failed to connect to BananaTray daemon'));
        }
    }

    _onDaemonVanished() {
        this._emitLog('daemon vanished from D-Bus');
        this._proxyGeneration++;
        this._clearProxy();
        this._onVanished?.();
    }

    _installProxy(proxy) {
        this._clearProxy();
        this._proxy = proxy;
        this._proxySignalId = this._proxy.connectSignal('RefreshComplete', (_proxy, _sender, args) => {
            const [jsonData] = args;
            this._onRefreshComplete(jsonData);
        });
    }

    _onRefreshComplete(jsonData) {
        try {
            this._emitSnapshot(parseSnapshot(jsonData));
        } catch (e) {
            this._emitError(`RefreshComplete parse error: ${e.message}`, _('Invalid quota data from BananaTray daemon'));
        }
    }

    _isCurrentProxy(proxy, generation) {
        return !this._destroyed && generation === this._proxyGeneration && this._proxy === proxy;
    }

    _clearProxy() {
        if (this._proxy && this._proxySignalId) {
            this._proxy.disconnectSignal(this._proxySignalId);
            this._proxySignalId = 0;
        }
        this._proxy = null;
    }

    _emitSnapshot(snapshot) {
        this._onSnapshot?.(snapshot);
    }

    _emitError(logMessage, uiMessage = null) {
        this._onError?.(logMessage, uiMessage);
    }

    _emitLog(message) {
        this._onError?.(message, null);
    }
}
