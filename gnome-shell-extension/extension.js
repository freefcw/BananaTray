/* BananaTray GNOME Shell Extension
 *
 * Displays AI coding assistant quota usage in a top bar popup.
 * Communicates with the BananaTray Rust daemon via D-Bus.
 *
 * GNOME 45+ ESM imports only.
 */

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {BananaTrayIndicator} from './panelButton.js';

export default class BananaTrayExtension extends Extension {
    enable() {
        this._indicator = new BananaTrayIndicator(this);
        Main.panel.addToStatusArea(this.uuid, this._indicator, 0, 'right');
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}
