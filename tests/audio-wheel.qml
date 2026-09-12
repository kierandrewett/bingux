import QtQuick
import QtTest
import Quickshell
import Quickshell.Io

ShellRoot {
    FileView {
        id: report
        path: Quickshell.env("BINGUX_CONTROL_TEST_RESULTS")
    }
    QtObject {
        id: outputAudio
        property real volume: 0.65
        property bool muted: false
    }
    QtObject {
        id: inputAudio
        property real volume: 0.4
        property bool muted: false
    }
    QtObject {
        id: outputNode
        property bool ready: true
        property var audio: outputAudio
    }
    QtObject {
        id: inputNode
        property bool ready: true
        property var audio: inputAudio
    }
    FloatingWindow {
        id: window
        implicitWidth: 400
        implicitHeight: 160
        AudioLevel {
            id: output
            x: 16
            y: 16
            width: 368
            node: outputNode
            label: "Volume"
            iconName: "audio-volume-high-symbolic"
            maximum: 1.5
        }
        AudioLevel {
            id: input
            x: 16
            y: 80
            width: 368
            node: inputNode
            label: "Microphone"
            iconName: "audio-input-microphone-symbolic"
        }
        TestCase {
            when: window.visible
            function notch(slider, delta) {
                mouseWheel(slider, slider.width / 2, slider.height / 2, 0, delta, Qt.NoButton);
            }
            function test_steps() {
                report.setText("FAILURES 1\n");
                const speaker = findChild(output, "audioLevelVolume");
                const mic = findChild(input, "audioLevelVolume");
                waitForRendering(speaker);
                notch(speaker, 120);
                fuzzyCompare(outputAudio.volume, 0.70, 0.0001, "Speaker increases five percentage points");
                if (!Theme.reducedMotion) {
                    wait(40);
                    verify(speaker.value > 0.65 && speaker.value < 0.70, "Thumb interpolates smoothly after wheel input");
                }
                tryVerify(() => Math.abs(speaker.value - 0.70) < 0.0001, 500);
                notch(speaker, -120);
                fuzzyCompare(outputAudio.volume, 0.65, 0.0001);
                notch(mic, 120);
                fuzzyCompare(inputAudio.volume, 0.45, 0.0001, "Microphone uses the same step");
                notch(mic, -60);
                fuzzyCompare(inputAudio.volume, 0.45, 0.0001, "Partial notches accumulate");
                notch(mic, -60);
                fuzzyCompare(inputAudio.volume, 0.40, 0.0001);
                outputAudio.volume = 1.49;
                notch(speaker, 120);
                compare(outputAudio.volume, 1.5, "Output is clamped at boost limit");
                inputAudio.volume = 0.99;
                notch(mic, 120);
                compare(inputAudio.volume, 1, "Microphone is clamped at 100 percent");
                inputAudio.volume = 0.01;
                notch(mic, -120);
                compare(inputAudio.volume, 0);
                outputAudio.muted = true;
                notch(speaker, -120);
                verify(!outputAudio.muted, "Wheel adjustment unmutes");
                outputAudio.volume = 0.77;
                tryVerify(() => Math.abs(speaker.value - 0.77) < 0.0001, 500, "External volume updates still reach the slider");
                outputNode.ready = true;
                notch(speaker, 120);
                mousePress(speaker, speaker.width / 2, speaker.height / 2);
                verify(!speaker.wheelActive, "Dragging cancels the wheel tween");
                mouseRelease(speaker, speaker.width / 2, speaker.height / 2);
                for (const slider of [speaker, mic]) {
                    const audio = slider === speaker ? outputAudio : inputAudio;
                    for (const target of [0.49, 0.74, 0.99]) {
                        const x = slider.leftPadding + slider.handle.width / 2 + target / slider.to * (slider.availableWidth - slider.handle.width);
                        mouseClick(slider, x, slider.height / 2);
                        fuzzyCompare(audio.volume, Math.round(target / 0.05) * 0.05, 0.0001, "Pointer clicks snap to five-percent values");
                    }
                    mousePress(slider, slider.width / 2, slider.height / 2);
                    mouseMove(slider, slider.width * 0.73, slider.height / 2);
                    fuzzyCompare(audio.volume / 0.05, Math.round(audio.volume / 0.05), 0.0001, "Dragging stays on the same five-percent grid");
                    mouseRelease(slider, slider.width * 0.73, slider.height / 2);
                }
                outputAudio.volume = 0.77;
                outputNode.ready = false;
                notch(speaker, 120);
                fuzzyCompare(outputAudio.volume, 0.77, 0.0001, "Unavailable devices cannot change");
                report.setText("PASS five-point speaker/mic wheel steps; partial notches, limits, mute and live bindings\nFAILURES 0\n");
            }
        }
    }
}
