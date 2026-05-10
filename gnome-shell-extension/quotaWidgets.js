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

function createStatusBadge(text, level) {
    return createLabel({
        text,
        style_class: `bananatray-status-badge bananatray-status-badge-${normalizeStatusLevel(level)}`,
        y_align: Clutter.ActorAlign.CENTER,
    }, false);
}

function createTierBadge(tier) {
    return createLabel({
        text: tier.toUpperCase(),
        style_class: 'bananatray-tier-badge',
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

        this._provider = provider;
        this._expanded = false;

        const level = providerVisualLevel(provider);
        const connection = normalizeConnection(provider.connection);
        const quotas = sortedQuotas(provider);
        const isExpandable = connection === 'connected' && quotas.length >= 2;

        // -- Header row --
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

        // Meta 行：email
        const meta = this._providerMeta(provider, connection);
        if (meta) {
            titleBlock.add_child(createLabel({
                text: meta,
                style_class: 'bananatray-provider-meta',
                x_expand: true,
            }));
        }
        header.add_child(titleBlock);

        // Tier badge (colored pill)
        if (provider.account_tier) {
            header.add_child(createTierBadge(provider.account_tier));
        }

        // Status badge or connection label
        if (connection === 'connected') {
            header.add_child(createStatusBadge(statusBadgeLabel(level), level));
        } else {
            header.add_child(createLabel({
                text: connectionLabel(connection),
                style_class: `bananatray-connection-badge bananatray-connection-${connection}`,
                y_align: Clutter.ActorAlign.CENTER,
            }, false));
        }

        // Expand/collapse button for multi-quota providers
        if (isExpandable) {
            this._expandButton = new St.Button({
                style_class: 'bananatray-expand-button',
                y_align: Clutter.ActorAlign.CENTER,
                child: createLabel({text: '▸', y_align: Clutter.ActorAlign.CENTER}, false),
            });
            this._expandButton.connect('clicked', () => this._toggleExpanded(quotas));
            header.add_child(this._expandButton);
        }

        this.add_child(header);

        // -- Quota area --
        this._quotaContainer = new St.BoxLayout({
            style_class: 'bananatray-quota-list',
            vertical: true,
            x_expand: true,
        });
        this.add_child(this._quotaContainer);

        if (isExpandable) {
            // 折叠态：显示最差配额的摘要行
            this._buildCollapsedView(quotas);
        } else {
            // 单配额 / 非 connected：直接展示全部
            this._buildFullQuotaArea(provider, connection, quotas);
        }
    }

    _providerMeta(provider, connection) {
        const parts = [];
        if (connection === 'error' && provider.quotas?.length > 0)
            parts.push(_('Cached data'));
        if (provider.account_email)
            parts.push(provider.account_email);
        // tier 已经通过 badge 展示，不在 meta 中重复

        return parts.join(' · ');
    }

    _toggleExpanded(quotas) {
        this._expanded = !this._expanded;
        this._quotaContainer.destroy_all_children();

        if (this._expanded) {
            // 展开态
            this._expandButton.child.text = '▾';
            for (const quota of quotas)
                this._quotaContainer.add_child(new BananaTrayQuotaRow(quota));
        } else {
            // 折叠态
            this._expandButton.child.text = '▸';
            this._buildCollapsedView(quotas);
        }
    }

    _buildCollapsedView(quotas) {
        // 折叠态：显示最差配额的 display_text + extra count
        if (quotas.length === 0)
            return;

        const worst = quotas[0]; // sortedQuotas 按严重度降序
        const collapsedRow = new St.BoxLayout({
            vertical: false,
            x_expand: true,
            style_class: 'bananatray-quota-line',
        });

        collapsedRow.add_child(createLabel({
            text: worst.label || worst.quota_type_key || _('Quota'),
            style_class: 'bananatray-quota-label',
            x_expand: true,
            y_align: Clutter.ActorAlign.CENTER,
        }));
        collapsedRow.add_child(createLabel({
            text: worst.display_text || '',
            style_class: 'bananatray-collapsed-value',
            y_align: Clutter.ActorAlign.CENTER,
        }, false));

        if (quotas.length > 1) {
            collapsedRow.add_child(createLabel({
                text: `+${quotas.length - 1}`,
                style_class: 'bananatray-collapsed-extra',
                y_align: Clutter.ActorAlign.CENTER,
            }, false));
        }

        this._quotaContainer.add_child(collapsedRow);

        // 折叠态也显示 bar
        this._quotaContainer.add_child(createQuotaBar(worst));
    }

    _buildFullQuotaArea(provider, connection, quotas) {
        if (quotas.length === 0) {
            this._quotaContainer.add_child(createLabel({
                text: connection === 'refreshing' ? _('Refreshing quota data') : _('No quota data available'),
                style_class: 'bananatray-provider-empty',
                x_expand: true,
            }));
            return;
        }

        for (const quota of quotas)
            this._quotaContainer.add_child(new BananaTrayQuotaRow(quota));
    }
});
