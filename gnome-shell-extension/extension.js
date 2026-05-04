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
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

// ── Constants ──

const DBUS_ID = 'com.bananatray.Daemon';
const DBUS_PATH = '/com/bananatray/Daemon';

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

// ── Provider Row Widget ──

const BananaTrayProviderRow = GObject.registerClass(
class BananaTrayProviderRow extends St.BoxLayout {
    _init(provider) {
        super._init({
            style_class: 'bananatray-provider-row',
            vertical: false,
            x_expand: true,
        });

        // Status dot
        this._statusDot = new St.Widget({
            style_class: `bananatray-status-dot bananatray-status-${(provider.worst_status || 'green').toLowerCase()}`,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this.add_child(this._statusDot);

        // Provider name
        this.add_child(new St.Label({
            text: provider.display_name || provider.id,
            style_class: 'bananatray-provider-name',
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
        }));

        // Primary quota display
        const primaryQuota = provider.quotas?.[0];
        this.add_child(new St.Label({
            text: primaryQuota ? primaryQuota.display_text : '—',
            style_class: 'bananatray-quota-text',
            y_align: Clutter.ActorAlign.CENTER,
        }));
    }
});

// ── Main Indicator ──

const BananaTrayIndicator = GObject.registerClass(
class BananaTrayIndicator extends PanelMenu.Button {
    _init() {
        super._init(0.0, 'BananaTray', false);

        // Panel icon — colored dot showing worst status
        this._iconBin = new St.Bin({
            style_class: 'bananatray-status-dot bananatray-status-green',
        });
        this.add_child(this._iconBin);

        // D-Bus proxy
        this._proxy = null;
        this._busWatchId = null;

        // Build popup layout
        this._buildUI();

        // Start watching for the daemon
        this._watchDaemon();
    }

    _buildUI() {
        // Header
        const headerBox = new St.BoxLayout({
            style_class: 'bananatray-header',
            vertical: false,
            x_expand: true,
        });

        this._titleLabel = new St.Label({
            text: 'BananaTray',
            y_align: Clutter.ActorAlign.CENTER,
            x_expand: true,
        });
        headerBox.add_child(this._titleLabel);

        this._statusLabel = new St.Label({
            text: '',
            style_class: 'bananatray-header-status',
            y_align: Clutter.ActorAlign.CENTER,
        });
        headerBox.add_child(this._statusLabel);

        this._refreshButton = new St.Button({
            style_class: 'bananatray-refresh-button',
            label: '↻',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._refreshButton.connect('clicked', () => this._refreshAll());
        headerBox.add_child(this._refreshButton);

        this.menu.box.add_child(headerBox);

        // Scrollable provider list
        this._scrollView = new St.ScrollView({
            style_class: 'bananatray-scrollview vfade',
            overlay_scrollbars: true,
        });
        this._providerList = new St.BoxLayout({
            vertical: true,
        });
        this._scrollView.add_actor(this._providerList);
        this.menu.box.add_child(this._scrollView);

        // Loading placeholder
        this._loadingLabel = new St.Label({
            text: 'Waiting for BananaTray daemon…',
            style_class: 'bananatray-loading',
        });
        this.menu.box.add_child(this._loadingLabel);

        // Footer
        const footer = new St.BoxLayout({
            style_class: 'bananatray-footer',
        });
        const openFullViewButton = new St.Button({
            style_class: 'bananatray-open-full-view',
            label: 'Open Full View',
            x_expand: true,
            x_align: Clutter.ActorAlign.CENTER,
        });
        openFullViewButton.connect('clicked', () => this._openSettings());
        footer.add_child(openFullViewButton);
        this.menu.box.add_child(footer);

        // Initial state: show loading
        this._scrollView.hide();
    }

    _watchDaemon() {
        this._busWatchId = Gio.bus_watch_name(
            Gio.BusType.SESSION,
            DBUS_ID,
            Gio.BusNameWatcherFlags.NONE,
            () => this._onDaemonAppeared(),
            () => this._onDaemonVanished(),
        );
    }

    _onDaemonAppeared() {
        log('BananaTray: daemon appeared on D-Bus');

        try {
            const Proxy = Gio.DBusProxy.makeProxyWrapper(DBUS_INTERFACE_XML);
            this._proxy = new Proxy(
                Gio.DBus.session,
                DBUS_ID,
                DBUS_PATH
            );

            // Connect RefreshComplete signal — only signal we use
            this._proxy.connectSignal('RefreshComplete', (proxy, sender, args) => {
                const [jsonData] = args;
                this._onRefreshComplete(jsonData);
            });

            // Fetch initial data
            this._fetchQuotas();
        } catch (e) {
            log(`BananaTray: failed to create D-Bus proxy: ${e.message}`);
        }
    }

    _onDaemonVanished() {
        log('BananaTray: daemon vanished from D-Bus');
        this._proxy = null;
        this._showLoading('BananaTray daemon not running');
    }

    _fetchQuotas() {
        if (!this._proxy) return;

        try {
            const [jsonData] = this._proxy.GetAllQuotasSync();
            this._updateAllRows(JSON.parse(jsonData));
        } catch (e) {
            log(`BananaTray: GetAllQuotas failed: ${e.message}`);
            this._showError('Failed to fetch quota data');
        }
    }

    _refreshAll() {
        if (!this._proxy) return;

        try {
            const [jsonData] = this._proxy.RefreshAllSync();
            this._updateAllRows(JSON.parse(jsonData));
        } catch (e) {
            log(`BananaTray: RefreshAll failed: ${e.message}`);
        }
    }

    _openSettings() {
        if (!this._proxy) return;

        try {
            this._proxy.OpenSettingsSync();
        } catch (e) {
            log(`BananaTray: OpenSettings failed: ${e.message}`);
        }
    }

    _onRefreshComplete(jsonData) {
        try {
            const data = JSON.parse(jsonData);
            this._updateAllRows(data);
        } catch (e) {
            log(`BananaTray: RefreshComplete parse error: ${e.message}`);
        }
    }

    _updateAllRows(data) {
        if (!data || !data.providers) return;

        // Update header status
        if (data.header) {
            this._statusLabel.text = data.header.status_text || '';
        }

        // Rebuild provider rows
        this._providerList.destroy_all_children();
        for (const provider of data.providers) {
            this._providerList.add_child(new BananaTrayProviderRow(provider));
        }

        // Update panel icon
        this._updatePanelIcon(data.providers);

        // Show provider list, hide loading
        this._scrollView.show();
        this._loadingLabel.hide();
    }

    _updatePanelIcon(providers) {
        let worst = 'green';
        for (const p of providers) {
            const s = (p.worst_status || 'green').toLowerCase();
            if (s === 'red') { worst = 'red'; break; }
            if (s === 'yellow') { worst = 'yellow'; }
        }
        this._iconBin.style_class = `bananatray-status-dot bananatray-status-${worst}`;
    }

    _showLoading(text) {
        this._loadingLabel.text = text || 'Loading…';
        this._loadingLabel.style_class = 'bananatray-loading';
        this._loadingLabel.show();
        this._scrollView.hide();
    }

    _showError(text) {
        this._loadingLabel.text = text || 'Error';
        this._loadingLabel.style_class = 'bananatray-error';
        this._loadingLabel.show();
        this._scrollView.hide();
    }

    destroy() {
        if (this._busWatchId) {
            Gio.bus_unwatch_name(this._busWatchId);
            this._busWatchId = null;
        }
        this._proxy = null;
        super.destroy();
    }
});

// ── Extension Entry Points ──

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
