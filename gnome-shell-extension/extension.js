/* BananaTray GNOME Shell Extension
 *
 * Displays AI coding assistant quota usage in a top bar popup.
 * Communicates with the BananaTray Rust daemon via D-Bus.
 *
 * GNOME 45+ ESM imports only.
 */

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import Pango from 'gi://Pango';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {QuotaClient} from './quotaClient.js';

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

function normalizeStatusLevel(value) {
    const status = String(value || '').toLowerCase();
    return Object.prototype.hasOwnProperty.call(STATUS_ORDER, status) ? status : 'green';
}

function normalizeConnection(value) {
    const connection = String(value || '').toLowerCase();
    return Object.prototype.hasOwnProperty.call(CONNECTION_LABELS, connection) ? connection : 'disconnected';
}

function strongerStatus(left, right) {
    return STATUS_ORDER[left] >= STATUS_ORDER[right] ? left : right;
}

function providerVisualLevel(provider) {
    const connection = normalizeConnection(provider.connection);
    if (connection === 'error' && (!provider.quotas || provider.quotas.length === 0))
        return 'red';
    if (connection === 'refreshing' || connection === 'disconnected')
        return 'yellow';

    return normalizeStatusLevel(provider.worst_status);
}

function statusBadgeLabel(level) {
    switch (level) {
    case 'red':
        return 'OUT';
    case 'yellow':
        return 'LOW';
    default:
        return 'OK';
    }
}

function connectionLabel(connection) {
    return CONNECTION_LABELS[normalizeConnection(connection)];
}

function quotaRatio(quota) {
    if (typeof quota.bar_ratio === 'number' && Number.isFinite(quota.bar_ratio))
        return Math.max(0, Math.min(1, quota.bar_ratio));

    if (quota.limit > 0)
        return Math.max(0, Math.min(1, quota.used / quota.limit));

    return 0;
}

function sortedQuotas(provider) {
    return [...(provider.quotas || [])].sort((a, b) => {
        const byStatus =
            STATUS_ORDER[normalizeStatusLevel(b.status_level)] -
            STATUS_ORDER[normalizeStatusLevel(a.status_level)];
        if (byStatus !== 0)
            return byStatus;

        return quotaRatio(a) - quotaRatio(b);
    });
}

function providerInitials(provider) {
    const name = provider.display_name || provider.id || '?';
    const words = name.trim().split(/\s+/).filter(Boolean);
    if (words.length >= 2)
        return `${words[0][0]}${words[1][0]}`.toUpperCase();

    return name.slice(0, 2).toUpperCase();
}

function createLabel(params, ellipsize = true) {
    const label = new St.Label(params);
    if (ellipsize && label.clutter_text) {
        label.clutter_text.set({
            ellipsize: Pango.EllipsizeMode.END,
            single_line_mode: true,
        });
    }
    return label;
}

function createStatusDot(level) {
    return new St.Widget({
        style_class: `bananatray-status-dot bananatray-status-${normalizeStatusLevel(level)}`,
        y_align: Clutter.ActorAlign.CENTER,
    });
}

function createStatusBadge(text, level, extraClass = '') {
    return createLabel({
        text,
        style_class: `bananatray-status-badge bananatray-status-badge-${normalizeStatusLevel(level)} ${extraClass}`,
        y_align: Clutter.ActorAlign.CENTER,
    }, false);
}

function createQuotaBar(quota) {
    const ratio = quotaRatio(quota);
    const level = normalizeStatusLevel(quota.status_level);
    const fillWidth = Math.round(96 * ratio);
    const bar = new St.Widget({
        style_class: 'bananatray-quota-bar',
        x_expand: true,
    });
    const fill = new St.Widget({
        style_class: `bananatray-quota-bar-fill bananatray-quota-bar-fill-${level}`,
        style: `width: ${fillWidth}px;`,
    });

    bar.add_child(fill);
    return bar;
}

function summarizeProviders(providers) {
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

// -- Quota Row Widget --------------------------------------------------------

const BananaTrayQuotaRow = GObject.registerClass(
class BananaTrayQuotaRow extends St.BoxLayout {
    _init(quota) {
        super._init({
            style_class: 'bananatray-quota-row',
            vertical: true,
            x_expand: true,
        });

        const topLine = new St.BoxLayout({
            style_class: 'bananatray-quota-line',
            vertical: false,
            x_expand: true,
        });

        topLine.add_child(createLabel({
            text: quota.label || quota.quota_type_key || 'Quota',
            style_class: 'bananatray-quota-label',
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        }));
        topLine.add_child(createLabel({
            text: quota.display_text || '',
            style_class: 'bananatray-quota-value',
            y_align: Clutter.ActorAlign.CENTER,
        }, false));

        this.add_child(topLine);
        this.add_child(createQuotaBar(quota));
    }
});

// -- Provider Row Widget -----------------------------------------------------

const BananaTrayProviderRow = GObject.registerClass(
class BananaTrayProviderRow extends St.BoxLayout {
    _init(provider) {
        super._init({
            style_class: `bananatray-provider-row bananatray-provider-${providerVisualLevel(provider)}`,
            vertical: true,
            x_expand: true,
        });

        const level = providerVisualLevel(provider);
        const connection = normalizeConnection(provider.connection);

        const header = new St.BoxLayout({
            style_class: 'bananatray-provider-header',
            vertical: false,
            x_expand: true,
        });
        header.add_child(createStatusDot(level));
        header.add_child(createLabel({
            text: providerInitials(provider),
            style_class: 'bananatray-provider-avatar',
            y_align: Clutter.ActorAlign.CENTER,
        }, false));

        const titleBlock = new St.BoxLayout({
            style_class: 'bananatray-provider-title-block',
            vertical: true,
            x_expand: true,
        });
        titleBlock.add_child(createLabel({
            text: provider.display_name || provider.id,
            style_class: 'bananatray-provider-name',
            x_expand: true,
        }));

        const meta = this._providerMeta(provider, connection);
        if (meta) {
            titleBlock.add_child(createLabel({
                text: meta,
                style_class: 'bananatray-provider-meta',
                x_expand: true,
            }));
        }
        header.add_child(titleBlock);

        if (connection === 'connected') {
            header.add_child(createStatusBadge(statusBadgeLabel(level), level));
        } else {
            header.add_child(createLabel({
                text: connectionLabel(connection),
                style_class: `bananatray-connection-badge bananatray-connection-${connection}`,
                y_align: Clutter.ActorAlign.CENTER,
            }, false));
        }

        this.add_child(header);
        this._addQuotaArea(provider, connection);
    }

    _providerMeta(provider, connection) {
        const parts = [];
        if (connection === 'error' && provider.quotas?.length > 0)
            parts.push('Cached data');
        if (provider.account_email)
            parts.push(provider.account_email);
        if (provider.account_tier)
            parts.push(provider.account_tier);

        return parts.join(' · ');
    }

    _addQuotaArea(provider, connection) {
        const quotas = sortedQuotas(provider);
        if (quotas.length === 0) {
            this.add_child(createLabel({
                text: connection === 'refreshing' ? 'Refreshing quota data' : 'No quota data available',
                style_class: 'bananatray-provider-empty',
                x_expand: true,
            }));
            return;
        }

        const quotaList = new St.BoxLayout({
            style_class: 'bananatray-quota-list',
            vertical: true,
            x_expand: true,
        });

        for (const quota of quotas)
            quotaList.add_child(new BananaTrayQuotaRow(quota));

        this.add_child(quotaList);
    }
});

// -- Main Indicator ----------------------------------------------------------

const BananaTrayIndicator = GObject.registerClass(
class BananaTrayIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'BananaTray', false);

        this._extension = Extension.lookupByURL(import.meta.url);
        this._panelBox = new St.BoxLayout({
            style_class: 'bananatray-panel-indicator',
            y_align: Clutter.ActorAlign.CENTER,
        });

        this._panelIcon = this._createPanelIcon();
        this._panelDot = createStatusDot('green');
        this._panelSummaryLabel = createLabel({
            text: 'BT',
            style_class: 'bananatray-panel-summary',
            y_align: Clutter.ActorAlign.CENTER,
        }, false);

        this._panelBox.add_child(this._panelIcon);
        this._panelBox.add_child(this._panelDot);
        this._panelBox.add_child(this._panelSummaryLabel);
        this.add_child(this._panelBox);

        this._client = new QuotaClient({
            onReady: () => this._showLoading('Loading quota data'),
            onVanished: () => this._showLoading('BananaTray daemon not running', 'red', 'Offline'),
            onSnapshot: snapshot => this._updateAllRows(snapshot),
            onError: (logMessage, uiMessage) => this._handleClientError(logMessage, uiMessage),
        });

        this._buildUI();
        this._client.start();
    }

    _createPanelIcon() {
        const iconFile = this._extension.dir.resolve_relative_path('icons/bananatray-symbolic.svg');
        return new St.Icon({
            style_class: 'bananatray-panel-icon',
            gicon: new Gio.FileIcon({file: iconFile}),
            y_align: Clutter.ActorAlign.CENTER,
        });
    }

    _buildUI() {
        this.menu.box.style_class = 'bananatray-menu-box';

        const headerBox = new St.BoxLayout({
            style_class: 'bananatray-header',
            vertical: false,
            x_expand: true,
        });
        headerBox.add_child(this._createPanelIcon());

        const titleBlock = new St.BoxLayout({
            style_class: 'bananatray-header-title-block',
            vertical: true,
            x_expand: true,
        });
        this._titleLabel = createLabel({
            text: 'BananaTray',
            style_class: 'bananatray-title',
            x_expand: true,
        }, false);
        this._statusLabel = createLabel({
            text: 'Waiting for daemon',
            style_class: 'bananatray-header-status',
            x_expand: true,
        });
        titleBlock.add_child(this._titleLabel);
        titleBlock.add_child(this._statusLabel);
        headerBox.add_child(titleBlock);

        this._refreshButton = new St.Button({
            style_class: 'bananatray-icon-button',
            y_align: Clutter.ActorAlign.CENTER,
            child: new St.Icon({
                icon_name: 'view-refresh-symbolic',
                style_class: 'bananatray-button-icon',
            }),
        });
        this._refreshButton.connect('clicked', () => {
            this._setPanelState('yellow', 'Refreshing');
            this._statusLabel.text = 'Refreshing';
            this._client.refreshAll();
        });
        headerBox.add_child(this._refreshButton);

        this.menu.box.add_child(headerBox);

        this._summaryBox = new St.BoxLayout({
            style_class: 'bananatray-summary',
            vertical: false,
            x_expand: true,
        });
        this.menu.box.add_child(this._summaryBox);

        this._scrollView = new St.ScrollView({
            style_class: 'bananatray-scrollview vfade',
            overlay_scrollbars: true,
            x_expand: true,
        });
        this._providerList = new St.BoxLayout({
            style_class: 'bananatray-provider-list',
            vertical: true,
            x_expand: true,
        });
        this._scrollView.set_child(this._providerList);
        this.menu.box.add_child(this._scrollView);

        this._messageLabel = createLabel({
            text: 'Waiting for BananaTray daemon',
            style_class: 'bananatray-loading',
            x_expand: true,
        });
        this.menu.box.add_child(this._messageLabel);

        const footer = new St.BoxLayout({
            style_class: 'bananatray-footer',
            x_expand: true,
        });
        const openFullViewButton = new St.Button({
            style_class: 'bananatray-open-full-view',
            label: 'Open Full View',
            x_expand: true,
            x_align: Clutter.ActorAlign.CENTER,
        });
        openFullViewButton.connect('clicked', () => this._client.openSettings());
        footer.add_child(openFullViewButton);
        this.menu.box.add_child(footer);

        this._scrollView.hide();
        this._summaryBox.hide();
    }

    _handleClientError(logMessage, uiMessage) {
        log(`BananaTray: ${logMessage}`);
        if (uiMessage)
            this._showError(uiMessage);
    }

    _updateAllRows(data) {
        if (!data || !Array.isArray(data.providers))
            return;

        const providers = data.providers;
        const summary = summarizeProviders(providers);

        this._statusLabel.text = data.header?.status_text
            ? `${data.header.status_text} · ${summary.headerText}`
            : summary.headerText;
        this._rebuildSummary(summary);
        this._setPanelState(summary.worstLevel, summary.panelText);

        this._providerList.destroy_all_children();
        for (const provider of providers)
            this._providerList.add_child(new BananaTrayProviderRow(provider));

        if (providers.length === 0) {
            this._showMessage('No enabled providers', 'bananatray-loading');
            return;
        }

        this._messageLabel.hide();
        this._summaryBox.show();
        this._scrollView.show();
    }

    _rebuildSummary(summary) {
        this._summaryBox.destroy_all_children();
        this._summaryBox.add_child(this._createSummaryCell('Providers', String(summary.total)));
        this._summaryBox.add_child(this._createSummaryCell('Connected', String(summary.connected)));
        this._summaryBox.add_child(this._createSummaryCell('Attention', String(summary.attention), summary.attention > 0));
    }

    _createSummaryCell(label, value, attention = false) {
        const cell = new St.BoxLayout({
            style_class: attention ? 'bananatray-summary-cell bananatray-summary-cell-attention' : 'bananatray-summary-cell',
            vertical: true,
            x_expand: true,
        });
        cell.add_child(createLabel({
            text: value,
            style_class: 'bananatray-summary-value',
            x_align: Clutter.ActorAlign.CENTER,
        }, false));
        cell.add_child(createLabel({
            text: label,
            style_class: 'bananatray-summary-label',
            x_align: Clutter.ActorAlign.CENTER,
        }, false));
        return cell;
    }

    _setPanelState(level, text) {
        const statusLevel = normalizeStatusLevel(level);
        this._panelDot.style_class = `bananatray-status-dot bananatray-status-${statusLevel}`;
        this._panelSummaryLabel.text = text || 'BT';
    }

    _showLoading(text, level = 'yellow', panelText = 'Waiting') {
        this._statusLabel.text = text || 'Loading';
        this._setPanelState(level, panelText);
        this._showMessage(text || 'Loading', 'bananatray-loading');
    }

    _showError(text) {
        this._statusLabel.text = text || 'Error';
        this._setPanelState('red', 'Error');
        this._showMessage(text || 'Error', 'bananatray-error');
    }

    _showMessage(text, styleClass) {
        this._messageLabel.text = text;
        this._messageLabel.style_class = styleClass;
        this._messageLabel.show();
        this._summaryBox.hide();
        this._scrollView.hide();
    }

    destroy() {
        this._client?.destroy();
        this._client = null;
        this._extension = null;
        super.destroy();
    }
});

// -- Extension Entry Points --------------------------------------------------

export default class BananaTrayExtension extends Extension {
    enable() {
        this._indicator = new BananaTrayIndicator();
        Main.panel.addToStatusArea(this.uuid, this._indicator, 0, 'right');
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}
