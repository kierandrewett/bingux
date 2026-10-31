.pragma library
// Icon names that other programs send, mapped to the Bingux icon that stands for them.
// The shell's own code never uses these: it asks for the Bingux name (the files of
// shell/bingux/icons). scripts/ui-lint.py checks that every target here is such a file.
var names = {
    // gnome-settings-daemon's on-screen display and its volume and brightness keys.
    "audio-volume-overamplified-symbolic": "audio-volume-high-symbolic",
    "microphone-sensitivity-high-symbolic": "audio-input-microphone-symbolic",
    "microphone-sensitivity-medium-symbolic": "audio-input-microphone-symbolic",
    "microphone-sensitivity-low-symbolic": "audio-input-microphone-symbolic",
    "microphone-sensitivity-muted-symbolic": "audio-input-microphone-muted-symbolic",
    "keyboard-brightness-symbolic": "input-keyboard-symbolic",
    "microphone-disabled-symbolic": "audio-input-microphone-muted-symbolic",
    "touchpad-enabled-symbolic": "input-touchpad-symbolic",
    "touchpad-disabled-symbolic": "input-touchpad-symbolic",
    // The icon names BlueZ gives a device.
    "audio-headset-symbolic": "audio-headphones-symbolic",
    "tablet-symbolic": "input-tablet-symbolic",
    "camera-video-symbolic": "video-generic-symbolic",
};
