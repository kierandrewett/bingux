pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root
    function insetRadius(outerRadius, padding) {
        return Math.max(0, outerRadius - padding);
    }

    readonly property color background: "#f5161616"
    readonly property color surface: "#f0303030"
    // Keep surface tint and transition opacity separate. These values make
    // the panel readable while letting the blurred backdrop remain visible.
    readonly property real shellSurfaceOpacity: 0.5
    readonly property real popupSurfaceOpacity: 0.8
    readonly property real overlaySurfaceOpacity: 0.55
    readonly property color shellSurface: Qt.rgba(0.106, 0.106, 0.106, shellSurfaceOpacity)
    readonly property color popupSurface: Qt.rgba(0.188, 0.188, 0.188, popupSurfaceOpacity)
    readonly property color overlaySurface: Qt.rgba(0.188, 0.188, 0.188, overlaySurfaceOpacity)
    readonly property int shellRadius: cardRadius + 4
    // Nested cards sit on translucent popup surfaces; keep their tint light
    // enough that the wallpaper and compositor blur remain perceptible.
    readonly property color elevated: Qt.rgba(0.25, 0.25, 0.27, 0.28)
    readonly property color menuWidgetBackground: elevated
    readonly property int menuWidgetRadius: insetRadius(cardRadius, gap)
    readonly property color hover: Qt.rgba(0.42, 0.42, 0.44, 0.34)
    readonly property color pressed: Qt.rgba(0.52, 0.52, 0.54, 0.46)
    readonly property color selection: Qt.rgba(accent.r, accent.g, accent.b, 0.28)
    readonly property color border: "#ffffff"
    readonly property color panelOuterOutline: Qt.rgba(0, 0, 0, 0.5)
    // Shared inner edge used by bars, cards, dock/sidebar surfaces, and
    // controls. Match Gnoblin's native window border at 75% opacity.
    readonly property color outline: Qt.rgba(0.314, 0.314, 0.314, 0.75)
    readonly property color contentShadow: Qt.rgba(0, 0, 0, 0.34)
    readonly property color text: "#fafafa"
    readonly property color muted: "#b0b0b0"
    // Follow the same accent choices exposed by GNOME Settings. The setting
    // is read through gsettings so this also works when Bingux is hosted by a
    // different compositor or shell, and the small poll keeps changes live
    // without requiring a shell restart.
    property string gnomeAccentName: "blue"
    property string gnomeColorScheme: "prefer-dark"
    readonly property color accent: accentColorFor(gnomeAccentName)
    readonly property color textSelection: Qt.rgba(accent.r, accent.g, accent.b, 0.28)
    readonly property color success: "#8ff0a4"
    readonly property color warning: "#f9f06b"
    readonly property color danger: "#ff7b63"
    function usageColor(percent, normal = text) {
        if (typeof percent !== "number" || !isFinite(percent))
            return normal;
        return percent >= 95 ? danger : percent >= 80 ? warning : normal;
    }
    readonly property int spaceSmall: 4
    readonly property int gap: 8
    readonly property int padding: 12
    readonly property int popupPadding: 16
    readonly property int paddingLarge: 20
    readonly property int radius: 12
    readonly property int cardRadius: 20
    readonly property int barHeight: 32
    // Keep the bar, sidebar, and joining corners identical over any wallpaper.
    readonly property color barBackground: shellSurface
    readonly property color barDivider: "#20ffffff"
    readonly property int barControlPadding: 6
    readonly property int activityIndicatorPadding: 12
    readonly property int barPrimaryPadding: 16
    readonly property int barControlGap: 2
    readonly property int barControlInset: 3
    readonly property int barControlRadius: 5
    readonly property int barIconTarget: 32
    readonly property int barEdgeHitWidth: iconSize + barPrimaryPadding * 2
    readonly property int controlHeight: 32
    readonly property int compactTilePadding: 12
    readonly property int controlTilePadding: 16
    readonly property int sliderControlHeight: 36
    readonly property int sliderTrackHeight: 6
    readonly property int sliderActiveTrackHeight: 8
    readonly property int sliderHandleSize: 20
    readonly property int sliderThumbSize: 16
    readonly property int iconSize: 16
    readonly property int fontSmall: 12
    readonly property int tooltipDelay: tooltipsWarm ? 0 : 500
    readonly property bool tooltipsWarm: tooltipOwners.length > 0 || tooltipCooldown.running
    property var tooltipOwners: []
    property Timer tooltipCooldown: Timer {
        interval: 1000
    }
    function beginTooltip(owner) {
        tooltipCooldown.stop();
        if (!tooltipOwners.includes(owner))
            tooltipOwners = tooltipOwners.concat([owner]);
    }
    function endTooltip(owner) {
        if (!tooltipOwners.includes(owner))
            return;
        tooltipOwners = tooltipOwners.filter(item => item !== owner);
        if (tooltipOwners.length === 0)
            tooltipCooldown.restart();
    }
    readonly property int tooltipMotion: reducedMotion ? 0 : 160
    readonly property real tooltipHiddenScale: 0.96
    readonly property color tooltipSurface: Qt.rgba(0.12, 0.12, 0.13, 0.82)
    readonly property color tooltipOutline: Qt.rgba(1, 1, 1, 0.16)
    readonly property int tooltipRadius: 12
    readonly property int tooltipHorizontalPadding: 12
    readonly property int tooltipVerticalPadding: 10
    readonly property int tooltipRowSpacing: 6
    readonly property int tooltipIconSize: 20
    readonly property int fontSize: 14
    readonly property int fontHeading: 16
    readonly property string fontFamily: "Adwaita Sans"
    readonly property int dockHeight: 88
    readonly property int dockPadding: Math.max(0, padding - spaceSmall)
    readonly property int dockExclusiveHeight: dockHeight + dockPadding * 2
    readonly property int dockItemSize: 72
    readonly property int dockIconSize: 56
    readonly property int dockBadgeSize: 24
    readonly property int audioBadgeFadeOut: 2000
    readonly property int mediaArtMotion: 220
    readonly property int mediaActionMotion: 180
    readonly property color recordingIndicator: "#c01c28"
    readonly property color privacyIndicator: "#ff7800"
    readonly property color notificationBadge: "#e52f3c"
    readonly property int notificationWidth: 384
    readonly property int notificationPadding: 16
    readonly property int notificationRadius: 16
    readonly property int notificationIconSize: 32
    readonly property int notificationHeaderHeight: 32
    readonly property int notificationSpacing: 4
    readonly property bool reducedMotion: Quickshell.env("BINGUX_REDUCED_MOTION") === "1"
    readonly property int notificationMotion: reducedMotion ? 0 : 420
    readonly property int notificationGroupMotion: reducedMotion ? 0 : 260
    readonly property int notificationStackMotion: reducedMotion ? 0 : 320
    readonly property real notificationEntranceOpacity: reducedMotion ? 1 : 0.65
    readonly property int notificationDismissDistance: 48
    readonly property int popupOpenMotion: reducedMotion ? 0 : 160
    readonly property int popupCloseMotion: reducedMotion ? 0 : 120
    readonly property real popupInitialScale: 0.9
    readonly property int motion: 120
    readonly property int statusIndicatorMotion: reducedMotion ? 0 : 180
    readonly property int osdWidth: 360
    readonly property int osdPadding: 16
    readonly property int osdIconSize: 24
    readonly property int osdIconTileSize: 48
    readonly property int osdPercentSize: 20
    readonly property int osdCloseMotion: reducedMotion ? 0 : 180
    readonly property int osdLevelMotion: reducedMotion ? 0 : 120
    readonly property int calendarMotion: reducedMotion ? 0 : 180
    readonly property int calendarPageMotion: reducedMotion ? 0 : 240
    readonly property int calendarAgendaHeight: 144
    readonly property int launchTimeout: 3000
    readonly property int dockLaunchTimeout: 20000
    // Search keeps its own density while using the same GNOME accent token as
    // the rest of the shell.
    readonly property color searchAccent: accent
    readonly property int searchInputHeight: 52
    readonly property int searchInputFontSize: 19
    readonly property int searchResultHeight: 50
    readonly property int searchCardRadius: 18
    readonly property int searchMotion: reducedMotion ? 0 : 90
    readonly property int searchExitMotion: reducedMotion ? 0 : 65
    readonly property int previewMotion: reducedMotion ? 0 : 180
    readonly property int previewOpenMotion: reducedMotion ? 0 : 200
    readonly property int previewCloseMotion: reducedMotion ? 0 : 150

    function accentColorFor(name) {
        const base = ({
                blue: Qt.rgba(0.208, 0.518, 0.894, 1),
                teal: Qt.rgba(0.129, 0.565, 0.643, 1),
                green: Qt.rgba(0.227, 0.580, 0.290, 1),
                yellow: Qt.rgba(0.784, 0.533, 0.000, 1),
                orange: Qt.rgba(0.929, 0.357, 0.000, 1),
                red: Qt.rgba(0.902, 0.176, 0.259, 1),
                pink: Qt.rgba(0.835, 0.380, 0.600, 1),
                purple: Qt.rgba(0.569, 0.255, 0.675, 1),
                slate: Qt.rgba(0.435, 0.514, 0.588, 1)
            })[name] || Qt.rgba(0.208, 0.518, 0.894, 1);
        if (gnomeColorScheme !== "prefer-dark")
            return base;
        // Dark Adwaita uses a softer, lighter version of the same hue so
        // focus rings and selected controls remain legible without becoming
        // an overly saturated blue block.
        return Qt.rgba(base.r + (1 - base.r) * 0.22, base.g + (1 - base.g) * 0.22, base.b + (1 - base.b) * 0.22, 1);
    }

    function updateGnomeAccent(value) {
        const name = value.trim().replace(/^['"]|['"]$/g, "");
        if (["blue", "teal", "green", "yellow", "orange", "red", "pink", "purple", "slate"].includes(name))
            gnomeAccentName = name;
    }

    function updateGnomeColorScheme(value) {
        const scheme = value.trim().replace(/^['"]|['"]$/g, "");
        if (["default", "prefer-light", "prefer-dark"].includes(scheme))
            gnomeColorScheme = scheme;
    }

    property Process accentReader: Process {
        id: accentReader
        command: ["gsettings", "get", "org.gnome.desktop.interface", "accent-color"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: root.updateGnomeAccent(text)
        }
    }

    property Process colorSchemeReader: Process {
        id: colorSchemeReader
        command: ["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: root.updateGnomeColorScheme(text)
        }
    }

    property Timer accentPoll: Timer {
        id: accentPoll
        interval: 1000
        repeat: true
        running: true
        onTriggered: {
            if (!accentReader.running)
                accentReader.running = true;
            if (!colorSchemeReader.running)
                colorSchemeReader.running = true;
        }
    }
}
