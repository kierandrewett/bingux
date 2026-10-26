import QtQuick
import ".."

// The visual settings an authentication screen may take from an account.
//
// Everything here is whitelisted and validated: apply() takes the published
// appearance record (`color_scheme`, `accent`, `clock_format`,
// `reduce_motion`, as bsm sends it) and keeps only values of the expected
// shape. Anything else, including a missing record, leaves the system
// defaults. Nothing in a record is ever evaluated or loaded.
//
// The standalone lock screen uses the defaults: the system accent, the
// locale's clock and the shell's own reduced-motion setting.
QtObject {
    id: root

    // Validated inputs. Set them through apply().
    property string colorScheme: "dark"
    property string accentHex: ""
    property string clockFormat: "auto"
    property bool reduceMotionRequested: false
    // True while a wallpaper picture is on screen. Over a picture the screen
    // keeps its white ink on a dark scrim, whatever the scheme.
    property bool wallpaperShown: false

    readonly property bool light: colorScheme === "light"
    readonly property color accent: accentHex !== "" ? Qt.color(accentHex) : Theme.accent
    readonly property bool twelveHourClock: clockFormat === "12h" ? true : clockFormat === "24h" ? false : /\bAP?\b|a$/i.test(Qt.locale().timeFormat(Locale.ShortFormat))
    readonly property bool reduceMotion: reduceMotionRequested || Theme.reducedMotion

    // Without a wallpaper the dark scheme tints deeply and keeps white ink; the
    // light scheme tints softly and switches to dark ink so text stays legible.
    readonly property bool darkInk: light && !wallpaperShown
    readonly property color ink: darkInk ? "#1d1d20" : "#ffffff"
    readonly property color inkMuted: darkInk ? Qt.rgba(0.114, 0.114, 0.125, 0.72) : Qt.rgba(1, 1, 1, 0.72)
    readonly property color glass: darkInk ? Qt.rgba(1, 1, 1, 0.55) : Qt.rgba(1, 1, 1, 0.16)
    readonly property color glassOutline: darkInk ? Qt.rgba(0, 0, 0, 0.16) : Qt.rgba(1, 1, 1, 0.22)
    readonly property color glassOutlineFocus: darkInk ? Qt.rgba(0, 0, 0, 0.5) : Qt.rgba(1, 1, 1, 0.6)
    readonly property color danger: darkInk ? "#c01c28" : Theme.danger
    readonly property color tintTop: light ? Qt.tint("#f4f5f8", Qt.rgba(accent.r, accent.g, accent.b, 0.3)) : Qt.darker(accent, 1.7)
    readonly property color tintBottom: light ? Qt.tint("#dfe2ea", Qt.rgba(accent.r, accent.g, accent.b, 0.45)) : Qt.darker(accent, 3.6)

    // Motion lengths. Zero when the account or the shell asks for reduced
    // motion: the same checks run, the animation collapses to one frame.
    readonly property int enterMotion: reduceMotion ? 0 : Theme.sessionEnterMotion
    readonly property int exitMotion: reduceMotion ? 0 : Theme.sessionExitMotion
    readonly property int motion: reduceMotion ? 0 : Theme.motion
    // Cross-fade between two accounts' wallpapers and accents.
    readonly property int fadeMotion: reduceMotion ? 0 : 320

    function isAccent(value) {
        return typeof value === "string" && /^#[0-9a-fA-F]{6}$/.test(value);
    }

    // Take a published appearance record. Unknown or malformed fields fall back.
    function apply(appearance) {
        const valid = appearance !== null && typeof appearance === "object";
        colorScheme = valid && (appearance.color_scheme === "dark" || appearance.color_scheme === "light") ? appearance.color_scheme : "dark";
        accentHex = valid && isAccent(appearance.accent) ? appearance.accent.toLowerCase() : "";
        clockFormat = valid && (appearance.clock_format === "12h" || appearance.clock_format === "24h") ? appearance.clock_format : "auto";
        reduceMotionRequested = valid && appearance.reduce_motion === true;
    }
}
