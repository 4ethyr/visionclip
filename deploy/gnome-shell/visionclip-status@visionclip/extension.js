import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

const STATUS_STALE_MS = 5 * 60 * 1000;
const REFRESH_MS = 180;

class VisionClipIndicator extends PanelMenu.Button {
    constructor() {
        super(0.0, 'VisionClip Status');

        this._state = 'idle';
        this._phase = 0;
        this._statusPath = GLib.build_filenamev([
            GLib.get_user_state_dir(),
            'visionclip',
            'status.json',
        ]);

        this.add_style_class_name('visionclip-panel-button');

        this._listeningIcon = new St.BoxLayout({
            style_class: 'visionclip-listening-icon',
            y_align: Clutter.ActorAlign.CENTER,
            x_align: Clutter.ActorAlign.CENTER,
        });
        this._micIcon = new St.Icon({
            icon_name: 'audio-input-microphone-symbolic',
            style_class: 'system-status-icon visionclip-mic-icon',
        });
        this._listeningIcon.add_child(this._micIcon);
        this._dots = ['a', 'b', 'c'].map(name => {
            const dot = new St.Widget({
                style_class: `visionclip-listening-dot visionclip-listening-dot-${name}`,
            });
            dot.set_size(5, 5);
            dot.set_pivot_point(0.5, 0.5);
            this._listeningIcon.add_child(dot);
            return dot;
        });

        this._stopIcon = new St.Icon({
            icon_name: 'media-playback-stop-symbolic',
            style_class: 'system-status-icon visionclip-stop-icon',
        });

        this.add_child(this._listeningIcon);
        this.add_child(this._stopIcon);
        this._stopIcon.hide();
        this.hide();

        this.connect('button-press-event', () => {
            if (this._state === 'speaking') {
                this._stopSpeaking();
                return Clutter.EVENT_STOP;
            }
            return Clutter.EVENT_PROPAGATE;
        });

        this._refreshId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, REFRESH_MS, () => {
            this._readState();
            this._animate();
            return GLib.SOURCE_CONTINUE;
        });
    }

    destroy() {
        if (this._refreshId) {
            GLib.source_remove(this._refreshId);
            this._refreshId = 0;
        }
        super.destroy();
    }

    _readState() {
        let nextState = 'idle';
        try {
            const [ok, contents] = GLib.file_get_contents(this._statusPath);
            if (ok) {
                const text = new TextDecoder('utf-8').decode(contents);
                const parsed = JSON.parse(text);
                if (typeof parsed.updated_at_ms === 'number'
                    && Date.now() - parsed.updated_at_ms <= STATUS_STALE_MS
                    && ['idle', 'listening', 'speaking'].includes(parsed.state)) {
                    nextState = parsed.state;
                }
            }
        } catch (_) {
            nextState = 'idle';
        }

        if (nextState !== this._state) {
            this._setState(nextState);
        }
    }

    _setState(state) {
        this._state = state;
        if (state === 'idle') {
            this.hide();
            return;
        }

        this.show();
        if (state === 'speaking') {
            this._listeningIcon.hide();
            this._stopIcon.show();
        } else {
            this._stopIcon.hide();
            this._listeningIcon.show();
        }
    }

    _animate() {
        if (this._state !== 'listening') {
            return;
        }

        this._phase = (this._phase + 1) % 36;
        this._dots.forEach((dot, index) => {
            const wave = (this._phase + index * 8) % 36;
            const progress = Math.sin((wave / 36) * Math.PI);
            const scale = 0.72 + progress * 0.55;
            dot.opacity = 120 + Math.floor(progress * 135);
            dot.set_scale(scale, scale);
        });
    }

    _stopSpeaking() {
        const localVisionclip = GLib.build_filenamev([
            GLib.get_home_dir(),
            '.local',
            'bin',
            'visionclip',
        ]);
        try {
            GLib.spawn_async(
                null,
                [localVisionclip, '--stop-speaking'],
                null,
                GLib.SpawnFlags.SEARCH_PATH,
                null
            );
        } catch (_) {
            GLib.spawn_command_line_async('visionclip --stop-speaking');
        }
    }
}

export default class VisionClipStatusExtension extends Extension {
    enable() {
        this._indicator = new VisionClipIndicator();
        Main.panel.addToStatusArea(this.uuid, this._indicator, 0, 'right');
    }

    disable() {
        this._indicator?.destroy();
        this._indicator = null;
    }
}
