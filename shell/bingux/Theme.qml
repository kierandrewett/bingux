pragma Singleton
import QtQuick
import Quickshell

QtObject {
    function insetRadius(outerRadius, padding) {
        return Math.max(0, outerRadius - padding);
    }

    readonly property color background: "#f5161616"
    readonly property color surface: "#f0303030"
    readonly property color shellSurface: "#1b1b1b"
    readonly property color popupSurface: shellSurface
    readonly property int shellRadius: cardRadius + 4
    readonly property color elevated: "#383838"
    readonly property color menuWidgetBackground: elevated
    readonly property int menuWidgetRadius: insetRadius(cardRadius, gap)
    readonly property color hover: "#454545"
    readonly property color pressed: "#505050"
    readonly property color border: "#ffffff"
    readonly property color panelOuterOutline: Qt.rgba(0, 0, 0, 0.5)
    readonly property color outline: "#505050"
    readonly property color text: "#fafafa"
    readonly property color muted: "#b0b0b0"
    readonly property color accent: "#99c1f1"
    readonly property color selection: "#31445d"
    readonly property color textSelection: Qt.rgba(accent.r, accent.g, accent.b, 0.28)
    readonly property color success: "#8ff0a4"
    readonly property color warning: "#f9f06b"
    readonly property color danger: "#ff7b63"
    function usageColor(percent, normal = text) {
        if (typeof percent !== "number" || !isFinite(percent)) return normal;
        return percent >= 95 ? danger : percent >= 80 ? warning : normal;
    }
    readonly property int spaceSmall: 4
    readonly property int gap: 8
    readonly property int padding: 12
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
    // Search has its own density so the launcher can feel deliberate without
    // changing the proportions of the rest of the shell.
    readonly property color searchSurface: shellSurface
    readonly property color searchHover: "#302f3543"
    readonly property color searchSelection: "#5b4c6f9b"
    readonly property color searchAccent: "#a9c8f7"
    readonly property int searchInputHeight: 52
    readonly property int searchInputFontSize: 19
    readonly property int searchResultHeight: 50
    readonly property int searchCardRadius: 18
    readonly property int searchMotion: reducedMotion ? 0 : 90
    readonly property int searchExitMotion: reducedMotion ? 0 : 65
    readonly property int previewMotion: reducedMotion ? 0 : 180
    readonly property int previewOpenMotion: reducedMotion ? 0 : 200
    readonly property int previewCloseMotion: reducedMotion ? 0 : 150
}
