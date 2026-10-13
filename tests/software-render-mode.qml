import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    id: shellRoot
    readonly property var theme: Theme
    FileView {
        id: result
        path: Quickshell.env("BINGUX_SOFTWARE_RENDER_TEST_RESULT")
    }
    TestCase {
        name: "SoftwareRenderMode"
        when: true
        function test_override_and_palette() {
            const expected = Quickshell.env("BINGUX_EXPECT_SOFTWARE_RENDERING") === "1";
            compare(shellRoot.theme.softwareRendering, expected);
            compare(shellRoot.theme.reducedMotion, expected || Quickshell.env("BINGUX_REDUCED_MOTION") === "1");
            compare(shellRoot.theme.parseOutputRenderer('{"outputs":[{"renderer":{"accelerated":false}}]}'), true);
            compare(shellRoot.theme.parseOutputRenderer('{"outputs":[{"renderer":{"accelerated":true}}]}'), false);
            compare(shellRoot.theme.parseOutputRenderer("not json"), null);
            verify(shellRoot.theme.parseRendererString("OpenGL renderer string: llvmpipe (LLVM 21.1.8)"));
            verify(shellRoot.theme.parseRendererString("softpipe"));
            verify(shellRoot.theme.parseRendererString("swrast"));
            verify(shellRoot.theme.parseRendererString("Software Rasterizer"));
            verify(!shellRoot.theme.parseRendererString("AMD Radeon RX 7900"));
            shellRoot.theme.outputRendererResult('{"outputs":[{"renderer":{"accelerated":false}}]}');
            shellRoot.theme.finishRendererStringProbe("llvmpipe");
            compare(shellRoot.theme.softwareRendering, expected, "The explicit override wins over renderer probes");
            if (expected) {
                compare(shellRoot.theme.shellSurface.a, 1);
                compare(shellRoot.theme.popupSurface.a, 1);
                compare(shellRoot.theme.overlaySurface.a, 1);
                compare(shellRoot.theme.elevated.a, 1);
                compare(shellRoot.theme.tooltipSurface.a, 1);
            } else {
                verify(shellRoot.theme.shellSurface.a < 1);
                verify(shellRoot.theme.popupSurface.a < 1);
                verify(shellRoot.theme.overlaySurface.a < 1);
                verify(shellRoot.theme.elevated.a < 1);
                verify(shellRoot.theme.tooltipSurface.a < 1);
            }
        }
        function cleanupTestCase() {
            result.setText("FAILURES " + qtest_results.failCount + "\n");
            Qt.quit();
        }
    }
}
