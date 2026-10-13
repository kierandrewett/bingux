    // Test-only IPC and counters. This fragment is appended to a private copy
    // of the installed shell; production components and native surfaces stay intact.
    QtObject {
        id: nativeIdle
        property string generation: Date.now() + "-" + Math.random()
        property int barFrames: 0
        property int dockFrames: 0
        property int detailFrames: 0
        property var savedData: null
        property var savedPlayer: null
        readonly property var media: root.controlCentre?.movableWidgets.find(item => item.widgetId === "control-media") || null
        function setup(busName) {
            const centre = root.lazyControlCentre();
            if (!centre)
                return false;
            const player = centre.mediaPlayers.find(player => player.dbusName === busName);
            if (!player)
                return false;
            if (savedData === null) {
                savedData = JSON.parse(JSON.stringify(BinguxPreferences.data));
                savedPlayer = centre.selectedMediaPlayer;
            }
            centre.selectedMediaPlayer = player;
            const desktop = Object.assign({}, savedData.desktop);
            const layout = JSON.parse(JSON.stringify(desktop.layout || DesktopLayout.defaults()));
            for (const zone of Object.keys(layout))
                layout[zone] = layout[zone].filter(id => id !== "control-media");
            layout["top-left"] = ["control-media"];
            layout["top-center"] = ["clock"];
            layout["top-right"] = ["controls", "notifications"];
            desktop.layout = layout;
            desktop.metrics = false;
            desktop.clockSeconds = false;
            // Assign only the live view. Do not call saveDesktop or write settings.
            BinguxPreferences.data = Object.assign({}, savedData, {
                desktop
            });
            centre.visible = false;
            return true;
        }
        function restore() {
            if (media)
                media.detailPopup.visible = false;
            if (savedData !== null) {
                if (root.controlCentre)
                    root.controlCentre.selectedMediaPlayer = savedPlayer;
                BinguxPreferences.data = savedData;
                savedData = null;
            }
        }
        function snapshot() {
            const seek = media ? nativeIdleInput.findChild(media, "mediaSeek") : null;
            const title = media ? nativeIdleInput.findChild(media, "mediaTitle") : null;
            return {
                generation,
                wallMs: Date.now(),
                source: Qt.resolvedUrl("shell.qml").toString(),
                frames: {
                    bar: barFrames,
                    dock: dockFrames,
                    details: detailFrames
                },
                clock: timeLabel.text,
                reducedMotion: Theme.reducedMotion,
                mediaReady: !!media,
                barVisible: topBar.visible,
                dockVisible: dock.visible,
                dockReady: dock.startupReady,
                dockGroups: dock.appGroups.length,
                mediaVisible: !!media?.visible,
                inline: !!media?.inlineControls,
                mediaWindowIsBar: !!media && media.Window.window === topBar.contentItem.Window.window,
                busName: media?.player?.dbusName || "",
                playing: !!media?.player?.isPlaying,
                title: media?.player?.trackTitle || "",
                displayedTitle: title?.text || "",
                titleOverflow: title?.overflow ?? -1,
                marqueeOffset: title?.offset ?? -1,
                detailOpen: !!media?.detailPopup.visible,
                detailRetained: !!media?.detailPopup.retained,
                detailWindowMapped: !!media?.detailPopup.nativeWindow.visible,
                seekVisible: !!seek?.visible,
                seekValue: seek?.value ?? -1,
                ticks: media?.playheadFrame ?? -1,
                position: media?.position ?? -1,
                length: media?.player?.length ?? -1,
                geometry: {
                    barWidth: topBar.width,
                    barHeight: topBar.height,
                    dockWidth: dock.width,
                    dockHeight: dock.height,
                    mediaWidth: media?.width ?? 0,
                    mediaHeight: media?.height ?? 0
                }
            };
        }
        function click(name) {
            if (!media)
                return false;
            const names = {
                play: "mediaPlayPause",
                next: "mediaNext",
                previous: "mediaPrevious",
                art: "mediaArtButton",
                seek: "mediaSeek"
            };
            const item = nativeIdleInput.findChild(media, names[name] || "");
            if (!item || !item.visible || !item.enabled || item.width <= 0 || item.height <= 0)
                return false;
            nativeIdleInput.mouseMove(item, item.width * (name === "seek" ? 0.65 : 0.5), item.height / 2);
            nativeIdleInput.wait(50);
            nativeIdleInput.mouseClick(item, item.width * (name === "seek" ? 0.65 : 0.5), item.height / 2);
            // Leave the control after the click, so hover previews/tooltips do
            // not become a second source of animation in a measured interval.
            nativeIdleInput.mouseMove(topBar.contentItem, topBar.width - 1, topBar.height - 1);
            return true;
        }
    }
    Connections {
        target: topBar.contentItem.Window.window
        function onFrameSwapped() {
            nativeIdle.barFrames++;
        }
    }
    Connections {
        target: dock.contentItem.Window.window
        function onFrameSwapped() {
            nativeIdle.dockFrames++;
        }
    }
    Connections {
        target: nativeIdle.media?.detailPopup.nativeWindow.contentItem.Window.window ?? null
        function onFrameSwapped() {
            nativeIdle.detailFrames++;
        }
    }
    TestCase {
        id: nativeIdleInput
        name: "NativeMediaIdleInput"
        when: false
        function test_manual() {
        }
    }
    IpcHandler {
        target: "native-media-idle"
        function setup(busName: string): bool {
            return nativeIdle.setup(busName);
        }
        function snapshot(): string {
            return JSON.stringify(nativeIdle.snapshot());
        }
        function click(name: string): bool {
            return nativeIdle.click(name);
        }
        function close(): bool {
            if (!nativeIdle.media)
                return false;
            nativeIdle.media.detailPopup.visible = false;
            return true;
        }
        function restore(): void {
            nativeIdle.restore();
        }
    }
