// PanelMenu.Button 控制器：装配弹窗 UI、QuotaClient 回调和面板状态。

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

import {_} from './i18n.js';
import {QuotaClient} from './quotaClient.js';
import {normalizeStatusLevel, summarizeProviders} from './quotaPresentation.js';
import {BananaTrayProviderRow, createLabel, createStatusDot} from './quotaWidgets.js';

export const BananaTrayIndicator = GObject.registerClass(
class BananaTrayIndicator extends PanelMenu.Button {
    _init(extension) {
        super._init(0.0, 'BananaTray', false);

        this._extension = extension;
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
            onReady: () => this._showLoading(_('Loading quota data')),
            onVanished: () => this._showLoading(_('BananaTray daemon not running'), 'red', _('Offline')),
            onSnapshot: snapshot => this._updateAllRows(snapshot),
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

        this._refreshButton = new St.Button({
            style_class: 'bananatray-icon-button',
            y_align: Clutter.ActorAlign.CENTER,
            child: new St.Icon({
                icon_name: 'view-refresh-symbolic',
                style_class: 'bananatray-button-icon',
            }),
        });
        this._refreshButton.connect('clicked', () => {
            this._setPanelState('yellow', _('Refreshing'));
            this._statusLabel.text = _('Refreshing');
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
            text: _('Waiting for BananaTray daemon'),
            style_class: 'bananatray-loading',
            x_expand: true,
        });
        this.menu.box.add_child(this._messageLabel);

        const footer = new St.BoxLayout({
            style_class: 'bananatray-footer',
            x_expand: true,
        });
        const openSettingsButton = new St.Button({
            style_class: 'bananatray-open-settings',
            label: _('Open Settings'),
            x_expand: true,
            x_align: Clutter.ActorAlign.CENTER,
        });
        openSettingsButton.connect('clicked', () => this._client.openSettings());
        footer.add_child(openSettingsButton);
        this.menu.box.add_child(footer);

        this._scrollView.hide();
        this._summaryBox.hide();
    }

    _handleClientError(logMessage, uiMessage) {
        log(`BananaTray: ${logMessage}`);
        if (uiMessage)
            this._showError(uiMessage);
    }

    _handleClientLog(message) {
        log(`BananaTray: ${message}`);
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
            this._showMessage(_('No enabled providers'), 'bananatray-loading');
            return;
        }

        this._messageLabel.hide();
        this._summaryBox.show();
        this._scrollView.show();
    }

    _rebuildSummary(summary) {
        this._summaryBox.destroy_all_children();
        this._summaryBox.add_child(this._createSummaryCell(_('Providers'), String(summary.total)));
        this._summaryBox.add_child(this._createSummaryCell(_('Connected'), String(summary.connected)));
        this._summaryBox.add_child(this._createSummaryCell(_('Attention'), String(summary.attention), summary.attention > 0));
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

    _showLoading(text, level = 'yellow', panelText = _('Waiting')) {
        this._statusLabel.text = text || _('Loading');
        this._setPanelState(level, panelText);
        this._showMessage(text || _('Loading'), 'bananatray-loading');
    }

    _showError(text) {
        this._statusLabel.text = text || _('Error');
        this._setPanelState('red', _('Error'));
        this._showMessage(text || _('Error'), 'bananatray-error');
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
