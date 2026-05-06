// 可复用 GNOME Shell UI 组件：Provider 行、Quota 行和共享小部件。

import Clutter from 'gi://Clutter';
import GObject from 'gi://GObject';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {_} from './i18n.js';
import {
    connectionLabel,
    normalizeConnection,
    normalizeStatusLevel,
    providerInitials,
    providerVisualLevel,
    quotaRatio,
    sortedQuotas,
    statusBadgeLabel,
} from './quotaPresentation.js';

const MIN_VISIBLE_QUOTA_RATIO = 0.001;

export function createLabel(params, ellipsize = true) {
    const label = new St.Label(params);
    if (ellipsize && label.clutter_text) {
        label.clutter_text.set({
            ellipsize: Pango.EllipsizeMode.END,
            single_line_mode: true,
        });
    }
    return label;
}

export function createStatusDot(level) {
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
    const bar = new St.Widget({
        style_class: 'bananatray-quota-bar',
        x_expand: true,
        layout_manager: new Clutter.BinLayout(),
    });
    const fill = new St.Widget({
        style_class: `bananatray-quota-bar-fill bananatray-quota-bar-fill-${level}`,
        x_align: Clutter.ActorAlign.FILL,
        y_align: Clutter.ActorAlign.FILL,
        x_expand: true,
        y_expand: true,
    });

    // 按实际轨道宽度缩放，避免父布局拉伸后满额仍只填充固定像素。
    fill.set_pivot_point(0, 0.5);
    if (ratio <= MIN_VISIBLE_QUOTA_RATIO)
        fill.hide();
    else
        fill.set_scale(ratio, 1);

    bar.add_child(fill);
    return bar;
}

export const BananaTrayQuotaRow = GObject.registerClass(
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
            text: quota.label || quota.quota_type_key || _('Quota'),
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

export const BananaTrayProviderRow = GObject.registerClass(
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
            parts.push(_('Cached data'));
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
                text: connection === 'refreshing' ? _('Refreshing quota data') : _('No quota data available'),
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
