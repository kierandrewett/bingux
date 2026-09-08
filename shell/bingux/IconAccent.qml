import QtQuick
import Quickshell

Scope {
    id: root
    property string source: ""
    readonly property color color: pickColor(OsIcons.palettes[source] || [])
    readonly property color foreground: contrast(color, "#101010") >= contrast(color, Theme.text) ? "#101010" : Theme.text

    function luminance(color) {
        color = Qt.color(color);
        const linear = value => value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4);
        return 0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b);
    }
    function contrast(a, b) {
        const first = luminance(a), second = luminance(b);
        return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
    }
    function pickColor(colors) {
        let selected = Theme.accent, score = 0;
        for (const value of colors) {
            const candidate = Qt.color(value);
            if (candidate.a < 0.5 || candidate.hsvSaturation < 0.08) continue;
            const weight = candidate.hsvSaturation * (1 - Math.abs(candidate.hslLightness - 0.5));
            if (weight > score) { selected = candidate; score = weight; }
        }
        // Preserve the sampled hue while lifting dark icons off the player card.
        selected = Qt.rgba(selected.r, selected.g, selected.b, 1);
        for (let step = 0; step < 20 && contrast(selected, Theme.elevated) < 3; step++)
            selected = Qt.hsla(selected.hslHue, selected.hslSaturation, Math.min(0.95, selected.hslLightness + 0.025), 1);
        return selected;
    }
}
