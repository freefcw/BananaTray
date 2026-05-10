// PanelMenu.Button 控制器：装配弹窗 UI、QuotaClient 回调和面板状态。

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

import {_} from './i18n.js';
import {QuotaClient} from './quotaClient.js';
import {normalizeStatusKind, normalizeStatusLevel, summarizeProviders} from './quotaPresentation.js';
import {BananaTrayProviderRow, createLabel, createStatusDot} from './quotaWidgets.js';

export const BananaTrayIndicator = GObject.registerClass(
class BananaTrayIndicator extends PanelMenu.Button {
    _init(extension) {
        super._init(0.0, 'BananaTray', false);

        this._extension = extension;
        this._isRefreshing = false;
        this._pendingSnapshot = null;
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
        });

        this._panelBox.add_child(this._panelIcon);
        this._panelBox.add_child(this._panelDot);
        this._panelBox.add_child(this._panelSummaryLabel);
        this.add_child(this._panelBox);

        this._client = new QuotaClient({
            onReady: () => this._showLoading(_('Loading quota data')),
            onVanished: () => this._showLoading(_('BananaTray daemon not running'), 'red', _('Offline')),
            onSnapshot: snapshot => this._onSnapshotReceived(snapshot),
            onError: (logMessage, uiMessage) => this._handleClientError(logMessage, uiMessage),
            onLog: message => this._handleClientLog(message),
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
        this.menu.box.add_style_class_name('bananatray-menu-box');

        // -- Header --
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
            text: _('Waiting for daemon'),
            style_class: 'bananatray-header-status',
            x_expand: true,
        });
        titleBlock.add_child(this._titleLabel);
        titleBlock.add_child(this._statusLabel);
        headerBox.add_child(titleBlock);

        // Header status badge (color-coded)
        this._headerBadge = this._createHeaderBadge('stale', _('Waiting'));
        this._headerBadgeLabel = this._headerBadge.get_first_child();
        headerBox.add_child(this._headerBadge);

        this.menu.box.add_child(headerBox);

        // -- Scrollable provider list --
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

        // -- Message label (loading/error) --
        this._messageLabel = createLabel({
            text: _('Waiting for BananaTray daemon'),
            style_class: 'bananatray-loading',
            x_expand: true,
        });
        this.menu.box.add_child(this._messageLabel);

        // -- Footer: Sync Data + spacer + Settings --
        const footer = new St.BoxLayout({
            style_class: 'bananatray-footer',
            x_expand: true,
        });

        // Sync Data button：默认轻量，数据过期/离线/刷新中时再强调。
        this._syncButton = new St.Button({
            style_class: 'bananatray-footer-btn bananatray-sync-button',
            x_expand: false,
        });
        const syncContent = new St.BoxLayout({
            style_class: 'bananatray-footer-btn-content',
            vertical: false,
            y_align: Clutter.ActorAlign.CENTER,
        });
        syncContent.add_child(new St.Icon({
            icon_name: 'view-refresh-symbolic',
            style_class: 'bananatray-footer-btn-icon',
        }));
        this._syncLabel = createLabel({
            text: _('Sync Data'),
            y_align: Clutter.ActorAlign.CENTER,
        }, false);
        syncContent.add_child(this._syncLabel);
        this._syncButton.set_child(syncContent);
        this._syncButton.connect('clicked', () => {
            if (this._isRefreshing)
                return;
            this._isRefreshing = true;
            this._syncLabel.text = _('Refreshing');
            this._setSyncButtonState('syncing');
            this._setPanelState('yellow', _('Refreshing'));
            this._statusLabel.text = _('Refreshing');
            this._client.refreshAll();
        });
        footer.add_child(this._syncButton);

        // Spacer
        footer.add_child(new St.Widget({x_expand: true}));

        // Settings button
        const settingsButton = new St.Button({
            style_class: 'bananatray-footer-btn',
            x_expand: false,
        });
        const settingsContent = new St.BoxLayout({
            style_class: 'bananatray-footer-btn-content',
            vertical: false,
            y_align: Clutter.ActorAlign.CENTER,
        });
        settingsContent.add_child(new St.Icon({
            icon_name: 'preferences-system-symbolic',
            style_class: 'bananatray-footer-btn-icon',
        }));
        settingsContent.add_child(createLabel({
            text: _('Settings'),
            y_align: Clutter.ActorAlign.CENTER,
        }, false));
        settingsButton.set_child(settingsContent);
        settingsButton.connect('clicked', () => this._client.openSettings());
        footer.add_child(settingsButton);

        this.menu.box.add_child(footer);

        // Lazy rendering: only rebuild popup content when menu opens
        this.menu.connect('open-state-changed', (_menu, open) => {
            if (open && this._pendingSnapshot) {
                this._updateAllRows(this._pendingSnapshot);
                this._pendingSnapshot = null;
            }
        });

        this._scrollView.hide();
    }

    _createHeaderBadge(statusKind, text) {
        const kind = normalizeStatusKind(statusKind);
        const badge = new St.BoxLayout({
            style_class: `bananatray-header-badge bananatray-header-badge-${kind}`,
            vertical: false,
            y_align: Clutter.ActorAlign.CENTER,
        });
        const label = createLabel({
            text,
            style_class: 'bananatray-header-badge-text',
            y_align: Clutter.ActorAlign.CENTER,
        }, false);
        badge.add_child(label);
        return badge;
    }

    _updateHeaderBadge(statusKind, text) {
        if (!this._headerBadge)
            return;
        const kind = normalizeStatusKind(statusKind);
        this._headerBadge.style_class = `bananatray-header-badge bananatray-header-badge-${kind}`;
        if (this._headerBadgeLabel)
            this._headerBadgeLabel.text = text;
    }

    _handleClientError(logMessage, uiMessage) {
        log(`BananaTray: ${logMessage}`);
        this._isRefreshing = false;
        this._syncLabel.text = _('Sync Data');
        if (uiMessage)
            this._showError(uiMessage);
    }

    _handleClientLog(message) {
        log(`BananaTray: ${message}`);
    }

    _onSnapshotReceived(snapshot) {
        if (!snapshot || !Array.isArray(snapshot.providers))
            return;

        this._isRefreshing = false;
        this._syncLabel.text = _('Sync Data');

        // 始终更新面板指示器（状态点 + 摘要文字），即使弹窗关闭
        const summary = summarizeProviders(snapshot.providers);
        this._setPanelState(summary.panelLevel, summary.panelText);

        if (this.menu.isOpen) {
            this._updateAllRows(snapshot, summary);
        } else {
            this._pendingSnapshot = snapshot;
        }
    }

    _updateAllRows(data, precomputedSummary = null) {
        if (!data || !Array.isArray(data.providers))
            return;

        const providers = data.providers;
        const summary = precomputedSummary || summarizeProviders(providers);

        // Header badge 展示全局同步状态；副标题只保留紧凑 Provider 摘要，避免重复。
        this._statusLabel.text = summary.headerText;

        // Header badge (color-coded by status_kind)
        const statusKind = data.header?.status_kind || 'Stale';
        this._updateHeaderBadge(statusKind, data.header?.status_text || _('Unknown'));
        this._setSyncButtonState(statusKind);

        this._setPanelState(summary.panelLevel, summary.panelText);

        this._providerList.destroy_all_children();
        for (const [index, provider] of providers.entries())
            this._providerList.add_child(new BananaTrayProviderRow(provider, index === 0));

        if (providers.length === 0) {
            this._showMessage(_('No enabled providers'), 'bananatray-loading');
            return;
        }

        this._messageLabel.hide();
        this._scrollView.show();
    }

    _setPanelState(level, text) {
        const statusLevel = normalizeStatusLevel(level);
        this._panelDot.style_class = `bananatray-status-dot bananatray-status-${statusLevel}`;
        this._panelSummaryLabel.text = text || 'BT';
    }

    _setSyncButtonState(statusKind) {
        if (!this._syncButton)
            return;
        const kind = normalizeStatusKind(statusKind);
        const emphasize = kind === 'stale' || kind === 'offline' || kind === 'syncing';
        this._syncButton.style_class = emphasize
            ? `bananatray-footer-btn bananatray-sync-button bananatray-sync-button-${kind}`
            : 'bananatray-footer-btn bananatray-sync-button';
    }

    _showLoading(text, level = 'yellow', panelText = _('Waiting')) {
        this._statusLabel.text = text || _('Loading');
        this._setPanelState(level, panelText);
        this._updateHeaderBadge(level === 'red' ? 'offline' : 'syncing', text || _('Loading'));
        this._setSyncButtonState(level === 'red' ? 'offline' : 'syncing');
        this._showMessage(text || _('Loading'), 'bananatray-loading');
    }

    _showError(text) {
        this._statusLabel.text = text || _('Error');
        this._setPanelState('red', _('Error'));
        this._updateHeaderBadge('offline', text || _('Error'));
        this._setSyncButtonState('offline');
        this._showMessage(text || _('Error'), 'bananatray-error');
    }

    _showMessage(text, styleClass) {
        this._messageLabel.text = text;
        this._messageLabel.style_class = styleClass;
        this._messageLabel.show();
        this._scrollView.hide();
    }

    destroy() {
        this._client?.destroy();
        this._client = null;
        this._extension = null;
        super.destroy();
    }
});
