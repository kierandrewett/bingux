    // App objects retain QObject identity, as foreign-toplevel updates do.
    QtObject {
        id: idleWindow
        property string appId: "idle-fixture"
        property string title: "Fixture"
        property var parent: null
        property var screens: []
        property bool activated: true
        property bool minimized: false
        function setRectangle(window, rectangle) {
        }
    }
    QtObject {
        id: idleManager
        property var activeToplevel: idleWindow
        property QtObject toplevels: QtObject {
            property var values: [idleWindow]
            signal objectInsertedPost(var object, int index)
            signal objectRemovedPost(var object, int index)
        }
    }
    // MPRIS extrapolates position when read, without notifying each frame.
    // Keep that behavior at the input seam; the complete media QML is real.
    QtObject {
        id: idlePlayer
        property bool playing: true
        property double start: Date.now()
        property double base: 42
        property string title: "Short title"
        property int seeks: 0
        property int playCalls: 0
        property int pauseCalls: 0
        property int nextCalls: 0
        property var source: ({
                identity: "Fixture",
                trackTitle: title,
                trackArtist: "Artist",
                trackArtUrl: "",
                positionSupported: true,
                lengthSupported: true,
                canControl: true,
                canSeek: true,
                canGoNext: true,
                canGoPrevious: true,
                canPlay: true,
                canPause: true,
                isPlaying: playing,
                length: 300,
                uniqueId: 1,
                get position() {
                    return idlePlayer.base + (idlePlayer.playing ? (Date.now() - idlePlayer.start) / 1000 : 0);
                },
                set position(value) {
                    idlePlayer.base = value;
                    idlePlayer.start = Date.now();
                    idlePlayer.seeks++;
                },
                positionChanged: () => {},
                play: () => {
                    idlePlayer.start = Date.now();
                    idlePlayer.playCalls++;
                    idlePlayer.playing = true;
                },
                pause: () => {
                    idlePlayer.base = idlePlayer.source.position;
                    idlePlayer.pauseCalls++;
                    idlePlayer.playing = false;
                },
                next: () => {
                    idlePlayer.nextCalls++;
                    idlePlayer.title = "Next track";
                },
                previous: () => {}
            })
    }
    FloatingWindow {
        id: idleMediaWindow
        implicitWidth: 600
        implicitHeight: 600
        visible: true
        property int frames: 0
        Connections {
            target: idleMediaWindow.contentItem.Window.window
            function onFrameSwapped() {
                idleMediaWindow.frames++;
            }
        }
        ControlCentreMedia {
            id: idleMedia
            width: 320
            barLayout: true
            barWindow: idleMediaWindow
            active: true
            player: idlePlayer.source
        }
    }
    FileView {
        id: idleReport
        path: Quickshell.env("BINGUX_IDLE_REPORT")
        blockWrites: true
    }
    TestCase {
        id: idleTest
        name: "ShellIdle"
        when: dock.startupReady && BinguxPreferences.loaded
        property string checks: ""
        function record(message) {
            checks += message + "\n";
            console.info(message);
        }
        function desktopOption(name, value) {
            const desktop = Object.assign({}, BinguxPreferences.data.desktop);
            desktop[name] = value;
            BinguxPreferences.data = Object.assign({}, BinguxPreferences.data, {
                desktop
            });
        }
        function sample(label, milliseconds) {
            const bar = topBar.idleFrames, icons = dock.idleFrames;
            const media = idleMediaWindow.frames, ticks = idleMedia.playheadFrame;
            wait(milliseconds);
            const result = {
                bar: topBar.idleFrames - bar,
                dock: dock.idleFrames - icons,
                media: idleMediaWindow.frames - media,
                ticks: idleMedia.playheadFrame - ticks
            };
            record(label + " " + JSON.stringify(result));
            return result;
        }
        function settle() {
            wait(800);
        }
        function initTestCase() {
            // No live usage graph is selected in the quiescent-shell case.
            desktopOption("metrics", false);
            desktopOption("clockSeconds", false);
            clockTimer.stop(); // Clock rollover is exercised explicitly below.
            settle();
            verify(!Theme.reducedMotion || Quickshell.env("BINGUX_REDUCED_MOTION") === "1");
        }
        function test_01_inline_playback_idle() {
            const seek = findChild(idleMedia, "mediaSeek");
            verify(!seek.visible, "The inline bar does not present a seek slider");
            const frames = sample("inline-playing-2s", 2100);
            compare(frames.ticks, 0, "Hidden seek panel must not sample the playhead");
            compare(frames.media, 0, "Short title and transport controls stay visually unchanged");
            compare(frames.bar, 0);
            compare(frames.dock, 0);
        }
        function test_02_real_music_updates() {
            const pause = findChild(idleMedia, "mediaPlayPause");
            verify(pause.enabled);
            mouseClick(pause);
            compare(idlePlayer.pauseCalls, 1);
            compare(idleMedia.player.isPlaying, false);
            tryCompare(pause, "text", "Play");
            mouseClick(pause);
            compare(idlePlayer.playCalls, 1);
            compare(idleMedia.player.isPlaying, true);
            mouseClick(findChild(idleMedia, "mediaNext"));
            compare(idlePlayer.nextCalls, 1);
            compare(idleMedia.player.trackTitle, "Next track");
            settle();
            compare(sample("inline-after-transport-1s", 1100).media, 0);
        }
        function test_03_reveal_seek_and_hide() {
            idleMedia.detailPopup.visible = true;
            const seek = findChild(idleMedia, "mediaSeek");
            tryCompare(seek, "visible", true);
            settle();
            const before = idleMedia.position;
            const frames = sample("visible-seek-1s", 1100);
            verify(frames.ticks > 0, "Visible playback keeps its existing refresh cadence");
            verify(idleMedia.position > before + 0.5);
            mouseClick(seek, seek.width * 0.6, seek.height / 2);
            verify(idlePlayer.seeks > 0, "Seeking still reaches the player");
            idleMedia.detailPopup.visible = false;
            tryCompare(seek, "visible", false);
            settle();
            compare(sample("seek-hidden-again-1s", 1100).ticks, 0);
            // Reopening while paused must sample position immediately, even
            // without a position signal or a running animation/one-second timer.
            idlePlayer.playing = false;
            idlePlayer.base = 123;
            idleMedia.detailPopup.visible = true;
            tryCompare(seek, "visible", true);
            tryCompare(idleMedia, "position", 123, 200);
            idleMedia.detailPopup.visible = false;
            settle();
        }
        function test_04_dock_snapshots_and_window_state() {
            const button = dock.testItems.itemAt(0);
            compare(button.currentGroup.windows[0], idleWindow);
            settle();
            const start = dock.idleFrames;
            for (let i = 0; i < 10; i++) {
                idleWindow.title = "Title " + i;
                dock.activity.activeStreams = [];
                notificationState.allEntries = [];
                dock.refreshAppGroups();
                wait(100);
            }
            compare(dock.testItems.itemAt(0), button, "Unchanged app identity retains its delegate");
            compare(button.currentGroup.windows[0].title, "Title 9");
            compare(dock.idleFrames - start, 0, "Equivalent snapshots do not repaint the dock");
            idleWindow.activated = false;
            tryCompare(button, "active", false);
            idleWindow.minimized = true;
            tryVerify(() => dock.minimisedWindowHistory.includes(idleWindow));
            idleWindow.minimized = false;
            tryVerify(() => !dock.minimisedWindowHistory.includes(idleWindow));
            idleWindow.activated = true;
            tryCompare(button, "active", true);
            settle();
        }
        function test_05_notifications_and_audio_badges() {
            const button = dock.testItems.itemAt(0);
            notificationState.allEntries = [
                {
                    notification: {
                        id: 101
                    },
                    appName: "Fixture",
                    desktopEntry: "idle-fixture",
                    summary: "New message",
                    body: "",
                    actions: [],
                    receivedAt: Date.now(),
                    timeoutMs: 0,
                    toastVisible: false
                }
            ];
            tryCompare(notificationButton, "count", 1);
            tryCompare(button, "notificationCount", 1);
            dock.activity.activeStreams = [
                {
                    properties: {
                        "application.id": "idle-fixture"
                    }
                }
            ];
            tryCompare(button, "playingAudio", true);
            settle();
            const frames = sample("settled-badges-1s", 1100);
            compare(frames.bar, 0);
            compare(frames.dock, 0);
            notificationState.allEntries = [];
            dock.activity.activeStreams = [];
            tryCompare(notificationButton, "count", 0);
            tryCompare(button, "notificationCount", 0);
            tryCompare(button, "playingAudio", false);
            settle();
        }
        function test_06_clock() {
            root.currentTime = new Date(2026, 9, 13, 12, 34, 0);
            tryCompare(timeLabel, "displayedText", root.currentTime.toLocaleTimeString(Qt.locale(), "hh:mm"));
            root.currentTime = new Date(2026, 9, 13, 12, 35, 0);
            tryCompare(timeLabel, "displayedText", root.currentTime.toLocaleTimeString(Qt.locale(), "hh:mm"));
            // Exercise the actual timer and settings connection, without waiting
            // a minute for a nondeterministic wall-clock boundary.
            desktopOption("clockSeconds", true);
            compare(clockTimer.interval, 1000);
            const before = root.currentTime.getTime();
            tryVerify(() => root.currentTime.getTime() > before, 1500);
            tryCompare(timeLabel, "displayedText", root.currentTime.toLocaleTimeString(Qt.locale(), "hh:mm:ss"));
            desktopOption("clockSeconds", false);
            verify(clockTimer.interval > 0 && clockTimer.interval <= 60020);
            clockTimer.stop();
            settle();
            const frames = sample("final-settled-1s", 1100);
            compare(frames.bar, 0);
            compare(frames.dock, 0);
            compare(frames.media, 0);
        }
        function cleanupTestCase() {
            idleReport.setText(checks + "FAILURES " + qtest_results.failCount + "\n");
            Qt.quit();
        }
    }
