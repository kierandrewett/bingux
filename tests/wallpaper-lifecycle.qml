import QtQuick
import QtTest
import Quickshell

ShellRoot {
    id: root
    Wallpaper {
        id: wallpaper
    }
    TestCase {
        name: "WallpaperLifecycle"
        when: true

        function capture(stage, name) {
            let saved = false;
            verify(stage.grabToImage(result => {
                saved = result.saveToFile(Quickshell.env("BINGUX_WALLPAPER_RESULTS") + "/" + name + ".png");
            }));
            tryVerify(() => saved, 3000);
        }

        function test_decode_and_crossfade() {
            tryVerify(() => wallpaper.testPanels.length > 0, 3000);
            const stage = wallpaper.testPanels[0].testStage;
            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_LANDSCAPE");
            tryVerify(() => stage.front.status === Image.Ready, 5000);
            tryCompare(stage.front, "opacity", 1, 3000);
            tryCompare(stage.front, "scale", 1, 3000);
            const image = stage.front;
            console.info("WALLPAPER_LOADED", stage.width, stage.height, image.sourceSize, image.implicitWidth, image.implicitHeight);
            capture(stage, "landscape");
            const bounded = image.sourceSize.width <= Math.ceil(stage.width * stage.testDpr * 1.06) && image.sourceSize.height <= Math.ceil(stage.height * stage.testDpr * 1.06);
            verify(image.sourceSize.width >= stage.width * stage.testDpr, "Decode must retain physical display resolution");
            wait(800); // Allow the runner to sample settled process memory.

            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_PORTRAIT");
            tryVerify(() => stage.front !== image && stage.front.status === Image.Ready, 5000);
            verify(image.source.toString() !== "", "Old image must survive the crossfade");
            tryCompare(stage.front, "opacity", 1, 2000);
            tryVerify(() => image.source.toString() === "", 2000);
            capture(stage, "portrait");

            BinguxPreferences.wallpaperUrl = "";
            tryVerify(() => stage.front.source.toString() === "" && stage.back.source.toString() === "", 2000);
            verify(bounded, "Decode must be bounded to display dimensions plus login zoom");
            console.info("WALLPAPER_LIFECYCLE_PASSED");
        }

        function test_interrupted_crossfade() {
            const stage = wallpaper.testPanels[0].testStage;
            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_LANDSCAPE");
            tryVerify(() => stage.front.status === Image.Ready, 5000);
            tryCompare(stage.front, "opacity", 1, 3000);
            const original = stage.front;
            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_PORTRAIT");
            tryVerify(() => stage.front !== original && stage.front.opacity > 0.1 && stage.front.opacity < 0.85, 3000);
            // Select the outgoing image again before the current fade finishes.
            // Opaque fixtures must continue covering the whole backdrop.
            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_LANDSCAPE");
            for (let frame = 0; frame < 80; frame++) {
                const coverage = 1 - (1 - stage.front.opacity) * (1 - stage.back.opacity);
                if (coverage < 0.999) {
                    console.error("WALLPAPER_INTERRUPTION_FAILED", frame, coverage);
                    capture(stage, "interrupted-gap");
                } else if ([0, 20, 79].includes(frame)) {
                    capture(stage, "interrupted-" + frame);
                }
                verify(coverage >= 0.999, "Interrupted fade exposed the backdrop: " + coverage);
                wait(16);
            }
            tryVerify(() => stage.front.source.toString() === BinguxPreferences.wallpaperUrl.toString(), 3000);
            tryCompare(stage.front, "opacity", 1, 3000);
            capture(stage, "interrupted-final");

            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_PORTRAIT");
            tryVerify(() => stage.front !== original && stage.front.opacity > 0.1 && stage.front.opacity < 0.85, 3000);
            BinguxPreferences.wallpaperUrl = "";
            wait(40);
            compare(stage.front.opacity, 0);
            compare(stage.back.opacity, 0);
            compare(stage.front.scale, 1);
            compare(stage.back.scale, 1);
            BinguxPreferences.wallpaperUrl = Quickshell.env("BINGUX_WALLPAPER_LANDSCAPE");
            tryVerify(() => stage.front.source.toString() === BinguxPreferences.wallpaperUrl.toString(), 3000);
            tryCompare(stage.front, "opacity", 1, 3000);
            wait(700);
            verify(stage.front.status === Image.Ready, "An obsolete fade completion cleared the replacement");
            console.info("WALLPAPER_INTERRUPTION_PASSED");
        }

        function cleanupTestCase() {
            Qt.quit();
        }
    }
}
