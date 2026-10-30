pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root
    readonly property string softwareRenderingOverride: Quickshell.env("BINGUX_SOFTWARE_RENDERING")
    property bool softwareRenderingValue: softwareRenderingOverride === "1"
    readonly property bool softwareRendering: softwareRenderingValue
    property bool rendererDetectionComplete: softwareRenderingOverride === "1" || softwareRenderingOverride === "0"
    property var sceneGraphSoftware: null
    property bool outputRendererFinished: false
    property bool outputRendererKnown: false
    property bool graphicsRendererKnown: false
    property bool rendererStringProbeStarted: false
    function setDetectedSoftwareRendering(value) {
        if (rendererDetectionComplete || value === null)
            return;
        softwareRenderingValue = value;
        rendererDetectionComplete = true;
    }
    function finishRendererDetection() {
        outputRendererFinished = true;
        if (rendererDetectionComplete || outputRendererKnown)
            return;
        if (sceneGraphSoftware === true) {
            setDetectedSoftwareRendering(true);
            return;
        }
        // Only the main shell window reports GraphicsInfo; the search, popout,
        // lock and capture shells are separate Quickshell processes that share
        // this singleton but never see it. Probe the renderer string in every
        // process rather than waiting for a report that may never come.
        if (!rendererStringProbeStarted) {
            rendererStringProbeStarted = true;
            rendererStringReader.running = true;
        }
    }
    function reportGraphicsApi(api) {
        if (rendererDetectionComplete || api === GraphicsInfo.Unknown)
            return;
        sceneGraphSoftware = api === GraphicsInfo.Software;
        graphicsRendererKnown = true;
        if (outputRendererFinished)
            finishRendererDetection();
    }
    function parseRendererString(value) {
        return /llvmpipe|softpipe|swrast|software rasterizer/i.test(value);
    }
    function finishRendererStringProbe(value) {
        if (rendererDetectionComplete || outputRendererKnown)
            return;
        setDetectedSoftwareRendering(parseRendererString(value));
    }
    function outputRendererResult(value) {
        const result = parseOutputRenderer(value);
        if (result !== null) {
            outputRendererKnown = true;
            setDetectedSoftwareRendering(result);
        }
    }
    function parseOutputRenderer(value) {
        try {
            const parsed = JSON.parse(value);
            const outputs = Array.isArray(parsed) ? parsed : parsed.outputs;
            if (!Array.isArray(outputs) || outputs.length === 0)
                return null;
            let known = false;
            for (const output of outputs) {
                if (typeof output?.renderer?.accelerated !== "boolean")
                    continue;
                known = true;
                if (!output.renderer.accelerated)
                    return true;
            }
            return known ? false : null;
        } catch (error) {
            return null;
        }
    }
    readonly property bool flatSurfaces: softwareRendering
    function flatten(foreground, backdrop) {
        return Qt.rgba(foreground.r * foreground.a + backdrop.r * (1 - foreground.a), foreground.g * foreground.a + backdrop.g * (1 - foreground.a), foreground.b * foreground.a + backdrop.b * (1 - foreground.a), 1);
    }
    function surfaceColor(foreground, backdrop) {
        return flatSurfaces ? flatten(foreground, backdrop) : foreground;
    }
    function insetRadius(outerRadius, padding) {
        return Math.max(0, outerRadius - padding);
    }

    readonly property color background: flatSurfaces ? "#ff161616" : "#f5161616"
    readonly property color surface: flatSurfaces ? "#ff303030" : "#f0303030"
    // Keep surface tint and transition opacity separate. These values make
    // the panel readable while letting the blurred backdrop remain visible.
    readonly property real shellSurfaceOpacity: 0.5
    readonly property real popupSurfaceOpacity: 0.8
    readonly property real overlaySurfaceOpacity: 0.55
    readonly property color shellSurface: flatSurfaces ? flatten(Qt.rgba(0.106, 0.106, 0.106, shellSurfaceOpacity), Qt.rgba(0.086, 0.086, 0.086, 1)) : Qt.rgba(0.106, 0.106, 0.106, shellSurfaceOpacity)
    readonly property color popupSurface: flatSurfaces ? flatten(Qt.rgba(0.188, 0.188, 0.188, popupSurfaceOpacity), Qt.rgba(0.106, 0.106, 0.106, 1)) : Qt.rgba(0.188, 0.188, 0.188, popupSurfaceOpacity)
    readonly property color overlaySurface: flatSurfaces ? flatten(Qt.rgba(0.188, 0.188, 0.188, overlaySurfaceOpacity), Qt.rgba(0.106, 0.106, 0.106, 1)) : Qt.rgba(0.188, 0.188, 0.188, overlaySurfaceOpacity)
    readonly property int shellRadius: cardRadius + 4
    // Nested cards sit on translucent popup surfaces; keep their tint light
    // enough that the wallpaper and compositor blur remain perceptible.
    readonly property color elevated: flatSurfaces ? flatten(Qt.rgba(0.25, 0.25, 0.27, 0.28), Qt.rgba(0.188, 0.188, 0.188, 1)) : Qt.rgba(0.25, 0.25, 0.27, 0.28)
    readonly property color menuWidgetBackground: elevated
    readonly property int menuWidgetRadius: insetRadius(cardRadius, gap)
    readonly property color hover: flatSurfaces ? flatten(Qt.rgba(0.42, 0.42, 0.44, 0.34), popupSurface) : Qt.rgba(0.42, 0.42, 0.44, 0.34)
    readonly property color pressed: flatSurfaces ? flatten(Qt.rgba(0.52, 0.52, 0.54, 0.46), popupSurface) : Qt.rgba(0.52, 0.52, 0.54, 0.46)
    readonly property color selection: flatSurfaces ? flatten(Qt.rgba(accent.r, accent.g, accent.b, 0.28), popupSurface) : Qt.rgba(accent.r, accent.g, accent.b, 0.28)
    readonly property color switcherSelection: surfaceColor(Qt.rgba(0.72, 0.74, 0.78, 0.20), popupSurface)
    readonly property color searchRowSeparator: surfaceColor(Qt.rgba(text.r, text.g, text.b, 0.08), overlaySurface)
    readonly property color searchSelectionOutline: surfaceColor(Qt.rgba(126 / 255, 159 / 255, 200 / 255, 56 / 255), popupSurface)
    readonly property real searchResultOpacity: flatSurfaces ? 1 : 0.58
    readonly property color captureWindowShade: surfaceColor(Qt.rgba(0, 0, 0, 153 / 255), Qt.rgba(0.20, 0.20, 0.20, 1))
    readonly property color captureRegionShade: surfaceColor(Qt.rgba(0, 0, 0, 136 / 255), Qt.rgba(0.20, 0.20, 0.20, 1))
    readonly property color recordingShade: surfaceColor(Qt.rgba(0, 0, 0, 51 / 255), Qt.rgba(0.20, 0.20, 0.20, 1))
    readonly property color captureHighlight: surfaceColor(Qt.rgba(accent.r, accent.g, accent.b, 0.30), popupSurface)
    readonly property color border: "#ffffff"
    readonly property color panelOuterOutline: flatSurfaces ? "#ff0b0b0b" : Qt.rgba(0, 0, 0, 0.5)
    // Shared inner edge used by bars, cards, dock/sidebar surfaces, and
    // controls. Match Gnoblin's native window border at 75% opacity.
    readonly property color outline: flatSurfaces ? flatten(Qt.rgba(0.314, 0.314, 0.314, 0.75), popupSurface) : Qt.rgba(0.314, 0.314, 0.314, 0.75)
    readonly property color contentShadow: flatSurfaces ? "transparent" : Qt.rgba(0, 0, 0, 0.34)
    readonly property color text: "#fafafa"
    readonly property color muted: "#b0b0b0"
    // Follow the same accent choices exposed by GNOME Settings. The setting
    // is read through gsettings so this also works when Bingux is hosted by a
    // different compositor or shell, and the small poll keeps changes live
    // without requiring a shell restart.
    property string gnomeAccentName: "blue"
    property string gnomeColorScheme: "prefer-dark"
    readonly property color accent: accentColorFor(gnomeAccentName)
    readonly property color textSelection: flatSurfaces ? flatten(Qt.rgba(accent.r, accent.g, accent.b, 0.28), popupSurface) : Qt.rgba(accent.r, accent.g, accent.b, 0.28)
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
    readonly property color barDivider: flatSurfaces ? flatten(Qt.rgba(1, 1, 1, 0.125), shellSurface) : "#20ffffff"
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
    readonly property color tooltipSurface: flatSurfaces ? flatten(Qt.rgba(0.12, 0.12, 0.13, 0.82), Qt.rgba(0.106, 0.106, 0.106, 1)) : Qt.rgba(0.12, 0.12, 0.13, 0.82)
    readonly property color tooltipOutline: flatSurfaces ? flatten(Qt.rgba(1, 1, 1, 0.16), popupSurface) : Qt.rgba(1, 1, 1, 0.16)
    readonly property int tooltipRadius: 12
    readonly property int tooltipHorizontalPadding: 12
    readonly property int tooltipVerticalPadding: 10
    readonly property int tooltipRowSpacing: 6
    readonly property int tooltipIconSize: 20
    readonly property int fontSize: 14
    readonly property int fontHeading: 16
    // `appearance.font_family` in bingux.toml; empty keeps Adwaita Sans.
    readonly property string fontFamily: (BinguxPreferences.data.appearance && BinguxPreferences.data.appearance.fontFamily) || "Adwaita Sans"
    readonly property int fontDisplay: 96
    readonly property int dockHeight: 88
    readonly property int dockPadding: Math.max(0, padding - spaceSmall)
    readonly property int dockExclusiveHeight: dockHeight + dockPadding * 2
    readonly property int dockItemSize: 72
    readonly property int dockIconSize: 56
    readonly property int dockBadgeSize: 24
    readonly property int audioBadgeFadeOut: 2000
    readonly property int mediaArtMotion: reducedMotion ? 0 : 220
    readonly property int mediaActionMotion: reducedMotion ? 0 : 180
    readonly property color recordingIndicator: "#c01c28"
    readonly property color privacyIndicator: "#ff7800"
    readonly property color notificationBadge: "#e52f3c"
    readonly property int notificationWidth: 384
    readonly property int notificationPadding: 16
    readonly property int notificationRadius: 16
    readonly property int notificationIconSize: 32
    readonly property int notificationHeaderHeight: 32
    readonly property int notificationSpacing: 4
    readonly property bool reducedMotion: Quickshell.env("BINGUX_REDUCED_MOTION") === "1" || softwareRendering
    readonly property int notificationMotion: reducedMotion ? 0 : 420
    readonly property int notificationGroupMotion: reducedMotion ? 0 : 260
    readonly property int notificationStackMotion: reducedMotion ? 0 : 320
    readonly property real notificationEntranceOpacity: reducedMotion ? 1 : 0.65
    readonly property int notificationDismissDistance: 48
    readonly property int popupOpenMotion: reducedMotion ? 0 : 160
    readonly property int popupCloseMotion: reducedMotion ? 0 : 120
    readonly property real popupInitialScale: 0.9
    readonly property int motion: reducedMotion ? 0 : 120
    readonly property int statusIndicatorMotion: reducedMotion ? 0 : 180
    // Lock screen and greeter: the entrance slides the clock and the account
    // into place; the exit fades them away before the desktop takes over.
    readonly property int sessionEnterMotion: reducedMotion ? 0 : 420
    readonly property int sessionExitMotion: reducedMotion ? 0 : 360
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
        running: !SessionContext.greeter && !SessionContext.kiosk
        stdout: StdioCollector {
            onStreamFinished: root.updateGnomeAccent(text)
        }
    }

    property Process colorSchemeReader: Process {
        id: colorSchemeReader
        command: ["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"]
        running: !SessionContext.greeter && !SessionContext.kiosk
        stdout: StdioCollector {
            onStreamFinished: root.updateGnomeColorScheme(text)
        }
    }

    // Probe Gnoblin after QML startup. Missing/old compositor APIs leave the
    // decision to the shell's own GraphicsInfo renderer report.
    property Process outputRendererReader: Process {
        command: ["gnoblinctl", "--json", "output", "list"]
        running: root.softwareRenderingOverride !== "1" && root.softwareRenderingOverride !== "0"
        stdout: StdioCollector {
            onStreamFinished: {
                root.outputRendererResult(text);
                root.finishRendererDetection();
            }
        }
    }

    // GraphicsInfo distinguishes Qt's pure software backend, but OpenGL can
    // still be backed by Mesa's llvmpipe. Query Xwayland asynchronously in
    // that case; an unavailable optional glxinfo leaves rendering unchanged.
    property Process rendererStringReader: Process {
        command: ["sh", "-c", "display=\"${DISPLAY:-}\"; auth=\"${XAUTHORITY:-}\"; if pid=$(pgrep -x Xwayland 2>/dev/null | head -n 1); then cmd=$(tr '\\0' '\\n' < /proc/$pid/cmdline 2>/dev/null); [ -n \"$display\" ] || display=$(printf '%s\\n' \"$cmd\" | awk '/^:[0-9]+$/{print;exit}'); [ -n \"$auth\" ] || auth=$(printf '%s\\n' \"$cmd\" | awk 'previous==\"-auth\"{print;exit}{previous=$0}'); fi; [ -n \"$display\" ] || exit 0; DISPLAY=\"$display\" XAUTHORITY=\"$auth\" timeout 3s glxinfo -B 2>/dev/null"]
        running: false
        stdout: StdioCollector {
            onStreamFinished: root.finishRendererStringProbe(text)
        }
    }

    // One long-lived monitor replaces re-reading both keys every second.
    property Process appearanceMonitor: Process {
        command: ["gsettings", "monitor", "org.gnome.desktop.interface"]
        running: !SessionContext.greeter && !SessionContext.kiosk
        stdout: SplitParser {
            onRead: line => {
                if (line.startsWith("accent-color:"))
                    root.updateGnomeAccent(line.slice(line.indexOf(":") + 1));
                else if (line.startsWith("color-scheme:"))
                    root.updateGnomeColorScheme(line.slice(line.indexOf(":") + 1));
            }
        }
        onExited: monitorRestart.restart()
    }

    property Timer monitorRestart: Timer {
        id: monitorRestart
        interval: 5000
        onTriggered: {
            appearanceMonitor.running = true;
            accentReader.running = true;
            colorSchemeReader.running = true;
        }
    }
}
