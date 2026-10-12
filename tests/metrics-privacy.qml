import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: result
        path: Quickshell.env("BINGUX_NOTES_TEST_RESULTS")
    }

    TestCase {
        name: "MetricsPrivacy"
        when: true

        function test_privacy_state_without_input_api() {
            const component = Qt.createComponent("Metrics.qml");
            compare(component.status, Component.Ready);
            const metrics = component.createObject(this);
            verify(metrics !== null);
            const now = Date.now();
            metrics.now = now;
            metrics.lastUpdatedAt = now;
            metrics.latest = {
                protocolVersion: 1,
                type: "metrics",
                cpuPercent: null,
                memoryTotalBytes: 100,
                memoryUsedBytes: 50,
                networkReceiveBytesPerSecond: null,
                networkTransmitBytesPerSecond: null,
                desktopStateAvailable: false,
                inputSources: [],
                currentInputSource: null,
                privacyAvailable: true,
                screenSharingAvailable: true,
                screenSharing: true,
                microphoneAvailable: false,
                microphoneInUse: false,
                locationAvailable: false,
                locationInUse: false
            };

            verify(metrics.isMetricsRecord(metrics.latest));
            compare(metrics.desktopStateAvailable, false);
            compare(metrics.privacyAvailable, true);
            compare(metrics.screenSharing, true);
            compare(metrics.microphoneAvailable, false);
            compare(metrics.locationAvailable, false);
            metrics.destroy();
        }

        function cleanupTestCase() {
            result.setText("FAILURES " + qtest_results.failCount + "\n");
            Qt.quit();
        }

        function test_microphone_capture_feed_validation_and_expiry() {
            const component = Qt.createComponent("Metrics.qml");
            compare(component.status, Component.Ready);
            const metrics = component.createObject(this);
            verify(metrics !== null);
            try {
                const base = {
                    protocolVersion: 1,
                    type: "metrics",
                    cpuPercent: null,
                    memoryTotalBytes: 100,
                    memoryUsedBytes: 50,
                    networkReceiveBytesPerSecond: null,
                    networkTransmitBytesPerSecond: null
                };
                verify(metrics.isMetricsRecord(base));
                const capture = {
                    app: "Recorder",
                    appId: "org.example.Recorder",
                    device: "Test input",
                    muted: false
                };
                const record = Object.assign({}, base, {
                    microphoneCapturesAvailable: true,
                    microphoneCaptures: [capture]
                });
                verify(metrics.isMetricsRecord(record));
                verify(!metrics.isMetricsRecord(Object.assign({}, record, {
                    microphoneCaptures: [
                        {
                            app: "Bad",
                            muted: false
                        }
                    ]
                })));
                verify(!metrics.isMetricsRecord(Object.assign({}, record, {
                    microphoneCapturesAvailable: "true"
                })));
                verify(!metrics.isMetricsRecord(Object.assign({}, record, {
                    microphoneCaptures: Array(33).fill(capture)
                })));
                verify(!metrics.isMetricsRecord(Object.assign({}, record, {
                    microphoneCaptures: [Object.assign({}, capture, {
                            app: "x".repeat(129)
                        })]
                })));
                verify(!metrics.isMetricsRecord(Object.assign({}, record, {
                    microphoneCaptures: [Object.assign({}, capture, {
                            device: "bad\nname"
                        })]
                })));
                metrics.now = Date.now();
                metrics.lastUpdatedAt = metrics.now;
                metrics.latest = record;
                compare(metrics.microphoneCapturesAvailable, true);
                compare(metrics.microphoneCaptureFeedPresent, true);
                compare(metrics.microphoneCaptures.length, 1);
                metrics.now += 6000;
                compare(metrics.microphoneCapturesAvailable, false);
                compare(metrics.microphoneCaptures.length, 0);
                compare(metrics.microphoneCaptureFeedPresent, false);
                metrics.now = metrics.lastUpdatedAt;
                metrics.latest = Object.assign({}, record, {
                    microphoneCaptures: []
                });
                compare(metrics.microphoneCapturesAvailable, true);
                compare(metrics.microphoneCaptures.length, 0);
                metrics.latest = Object.assign({}, record, {
                    microphoneCapturesAvailable: false,
                    microphoneCaptures: []
                });
                compare(metrics.microphoneCaptureFeedPresent, true);
                compare(metrics.microphoneCapturesAvailable, false);
                metrics.latest = base;
                compare(metrics.microphoneCapturesAvailable, false);
                compare(metrics.microphoneCaptureFeedPresent, false);
            } finally {
                metrics.destroy();
            }
        }
    }
}
