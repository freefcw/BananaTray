// 展示层纯函数：归一化 D-Bus 快照字段并生成面板/列表摘要。

import {_, ngettext} from './i18n.js';

const STATUS_ORDER = {
    green: 0,
    yellow: 1,
    red: 2,
};

const CONNECTION_KEYS = new Set(['connected', 'refreshing', 'error', 'disconnected']);

export function normalizeStatusLevel(value) {
    const status = String(value || '').toLowerCase();
    return status in STATUS_ORDER ? status : 'yellow';
}

export function normalizeConnection(value) {
    const connection = String(value || '').toLowerCase();
    return CONNECTION_KEYS.has(connection) ? connection : 'disconnected';
}

const STATUS_KIND_KEYS = new Set(['synced', 'syncing', 'stale', 'offline']);

export function normalizeStatusKind(value) {
    const kind = String(value || '').toLowerCase();
    return STATUS_KIND_KEYS.has(kind) ? kind : 'stale';
}

export function headerStatusText(statusKind, elapsedSecs) {
    switch (normalizeStatusKind(statusKind)) {
    case 'synced':
        return _('Synced');
    case 'syncing':
        return _('Syncing');
    case 'offline':
        return _('Offline');
    default:
        if (typeof elapsedSecs !== 'number' || !Number.isFinite(elapsedSecs))
            return null;

        const secs = Math.max(0, Math.floor(elapsedSecs));
        if (secs < 3600) {
            const minutes = Math.floor(secs / 60);
            return ngettext('%d minute ago', '%d minutes ago', minutes).replaceAll('%d', minutes);
        }

        const hours = Math.floor(secs / 3600);
        return ngettext('%d hour ago', '%d hours ago', hours).replaceAll('%d', hours);
    }
}

function primaryProviderPanelText(provider) {
    const primaryQuota = sortedQuotas(provider)[0];
    const name = provider.display_name || provider.id || _('Provider');
    return primaryQuota
        ? `${name} ${primaryQuota.display_text}`
        : `${name} ${connectionLabel(provider.connection)}`;
}

export function providerVisualLevel(provider) {
    const connection = normalizeConnection(provider.connection);
    if (connection === 'error' && (!provider.quotas || provider.quotas.length === 0))
        return 'red';
    if (connection === 'refreshing' || connection === 'disconnected')
        return 'yellow';

    return normalizeStatusLevel(provider.worst_status);
}

export function statusBadgeLabel(level) {
    switch (level) {
    case 'red':
        // Translators: quota status badge shown when quota is exhausted.
        return _('OUT');
    case 'yellow':
        // Translators: quota status badge shown when quota is low but not exhausted.
        return _('LOW');
    default:
        // Translators: quota status badge shown when quota usage is healthy.
        return _('OK');
    }
}

export function connectionLabel(connection) {
    switch (normalizeConnection(connection)) {
    case 'connected':
        return _('Connected');
    case 'refreshing':
        return _('Refreshing');
    case 'error':
        return _('Error');
    default:
        return _('Disconnected');
    }
}

export function quotaRatio(quota) {
    if (typeof quota.bar_ratio === 'number' && Number.isFinite(quota.bar_ratio))
        return Math.max(0, Math.min(1, quota.bar_ratio));

    if (quota.limit > 0)
        return Math.max(0, Math.min(1, quota.used / quota.limit));

    return 0;
}

export function sortedQuotas(provider) {
    return [...(provider.quotas || [])].sort((a, b) => {
        const byStatus =
            STATUS_ORDER[normalizeStatusLevel(b.status_level)] -
            STATUS_ORDER[normalizeStatusLevel(a.status_level)];
        if (byStatus !== 0)
            return byStatus;

        return quotaRatio(a) - quotaRatio(b);
    });
}

export function providerInitials(provider) {
    const name = provider.display_name || provider.id || '?';
    const words = name.trim().split(/\s+/).filter(Boolean);
    if (words.length >= 2)
        return `${words[0][0]}${words[1][0]}`.toUpperCase();

    return name.slice(0, 2).toUpperCase();
}

export function summarizeProviders(providers) {
    const summary = {
        total: providers.length,
        connected: 0,
        refreshing: 0,
        error: 0,
        disconnected: 0,
        attention: 0,
        panelLevel: 'green',
        panelText: _('No providers'),
        headerText: _('No enabled providers'),
    };

    for (const provider of providers) {
        const connection = normalizeConnection(provider.connection);
        const level = providerVisualLevel(provider);

        if (connection === 'connected')
            summary.connected += 1;
        else if (connection === 'refreshing')
            summary.refreshing += 1;
        else if (connection === 'error')
            summary.error += 1;
        else
            summary.disconnected += 1;

        if (connection !== 'connected' || level !== 'green')
            summary.attention += 1;
    }

    if (summary.total === 0)
        return summary;

    const headerParts = [
        formatProviderCount(summary.total),
        formatConnectedCount(summary.connected),
    ];
    if (summary.refreshing > 0)
        headerParts.push(formatRefreshingCount(summary.refreshing));
    if (summary.error > 0)
        headerParts.push(formatErrorCount(summary.error));
    if (summary.disconnected > 0)
        headerParts.push(formatOfflineCount(summary.disconnected));
    summary.headerText = headerParts.join(' · ');

    // providers 已由 daemon 按用户设置顺序排列；顶栏文字和状态点固定跟随第一个。
    summary.panelLevel = providerVisualLevel(providers[0]);
    summary.panelText = primaryProviderPanelText(providers[0]);

    return summary;
}

function formatProviderCount(count) {
    return ngettext('%d provider', '%d providers', count).replaceAll('%d', count);
}

function formatConnectedCount(count) {
    return ngettext('%d connected', '%d connected', count).replaceAll('%d', count);
}

function formatRefreshingCount(count) {
    return ngettext('%d refreshing', '%d refreshing', count).replaceAll('%d', count);
}

function formatErrorCount(count) {
    return ngettext('%d error', '%d errors', count).replaceAll('%d', count);
}

function formatOfflineCount(count) {
    return ngettext('%d offline', '%d offline', count).replaceAll('%d', count);
}
