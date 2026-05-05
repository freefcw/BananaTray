// 展示层纯函数：归一化 D-Bus 快照字段并生成面板/列表摘要。

const STATUS_ORDER = {
    green: 0,
    yellow: 1,
    red: 2,
};

const CONNECTION_LABELS = {
    connected: 'Connected',
    refreshing: 'Refreshing',
    error: 'Error',
    disconnected: 'Disconnected',
};

export function normalizeStatusLevel(value) {
    const status = String(value || '').toLowerCase();
    return Object.prototype.hasOwnProperty.call(STATUS_ORDER, status) ? status : 'green';
}

export function normalizeConnection(value) {
    const connection = String(value || '').toLowerCase();
    return Object.prototype.hasOwnProperty.call(CONNECTION_LABELS, connection) ? connection : 'disconnected';
}

function strongerStatus(left, right) {
    return STATUS_ORDER[left] >= STATUS_ORDER[right] ? left : right;
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
        return 'OUT';
    case 'yellow':
        return 'LOW';
    default:
        return 'OK';
    }
}

export function connectionLabel(connection) {
    return CONNECTION_LABELS[normalizeConnection(connection)];
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
        worstLevel: 'green',
        panelText: 'No providers',
        headerText: 'No enabled providers',
    };

    let worstProvider = null;
    let worstProviderLevel = 'green';

    for (const provider of providers) {
        const connection = normalizeConnection(provider.connection);
        const level = providerVisualLevel(provider);
        summary.worstLevel = strongerStatus(summary.worstLevel, level);

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

        if (!worstProvider || STATUS_ORDER[level] > STATUS_ORDER[worstProviderLevel]) {
            worstProvider = provider;
            worstProviderLevel = level;
        }
    }

    if (summary.total === 0)
        return summary;

    summary.headerText = `${summary.total} providers · ${summary.connected} connected`;
    if (summary.refreshing > 0)
        summary.headerText += ` · ${summary.refreshing} refreshing`;
    if (summary.error > 0)
        summary.headerText += ` · ${summary.error} error`;
    if (summary.disconnected > 0)
        summary.headerText += ` · ${summary.disconnected} offline`;

    if (summary.worstLevel === 'green') {
        summary.panelText = `${summary.connected}/${summary.total} OK`;
    } else if (worstProvider) {
        const primaryQuota = sortedQuotas(worstProvider)[0];
        const name = worstProvider.display_name || worstProvider.id || 'Provider';
        summary.panelText = primaryQuota
            ? `${name} ${primaryQuota.display_text}`
            : `${name} ${connectionLabel(worstProvider.connection)}`;
    }

    return summary;
}
