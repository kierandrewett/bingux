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
    }
}
