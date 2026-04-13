{ stdenvNoCC, lib }:
stdenvNoCC.mkDerivation {
    pname = "bingux-plymouth";
    version = "2.0.0";

    src = ../../files/plymouth/bingux;

    installPhase = ''
        runHook preInstall

        themeDir="$out/share/plymouth/themes/bingux"
        mkdir -p "$themeDir"

        # Copy all assets (animation frames, throbbers, watermark, dialog elements)
        cp -r "$src"/. "$themeDir"/

        # Remove the old spinner.plymouth if present, we generate our own
        rm -f "$themeDir/spinner.plymouth"

        # Write the theme config (Fedora bgrt style)
        cat > "$themeDir/bingux.plymouth" <<THEME
[Plymouth Theme]
Name=Bingux
Description=Bingux boot splash
ModuleName=two-step

[two-step]
Font=Adwaita Sans 12
TitleFont=Adwaita Sans 30
ImageDir=$themeDir
DialogHorizontalAlignment=.5
DialogVerticalAlignment=.382
TitleHorizontalAlignment=.5
TitleVerticalAlignment=.382
HorizontalAlignment=.5
VerticalAlignment=.7
WatermarkHorizontalAlignment=.5
WatermarkVerticalAlignment=.96
Transition=none
TransitionDuration=0.0
BackgroundStartColor=0x000000
BackgroundEndColor=0x000000
ProgressBarBackgroundColor=0x606060
ProgressBarForegroundColor=0xffffff
DialogClearsFirmwareBackground=true
MessageBelowAnimation=true

[boot-up]
UseEndAnimation=false
UseFirmwareBackground=true

[shutdown]
UseEndAnimation=false
UseFirmwareBackground=true

[reboot]
UseEndAnimation=false
UseFirmwareBackground=true

[updates]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Installing Updates...
SubTitle=Do not turn off your computer

[system-upgrade]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Upgrading System...
SubTitle=Do not turn off your computer

[firmware-upgrade]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Upgrading Firmware...
SubTitle=Do not turn off your computer

[system-reset]
SuppressMessages=true
ProgressBarShowPercentComplete=true
UseProgressBar=true
Title=Resetting System...
SubTitle=Do not turn off your computer
THEME

        runHook postInstall
    '';

    meta = {
        description = "Bingux Plymouth boot theme (Fedora-style BGRT with bingus branding)";
        license = lib.licenses.mit;
        platforms = lib.platforms.linux;
    };
}
