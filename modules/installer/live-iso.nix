{ lib, modulesPath, pkgs, ... }:
let
    bingux-plymouth = pkgs.callPackage ../../pkgs/bingux-plymouth { };
    bingux-installer = pkgs.callPackage ../../pkgs/bingux-installer { };

    # labwc config
    labwcRc = pkgs.writeText "labwc-rc.xml" ''
        <?xml version="1.0"?>
        <labwc_config>
            <theme>
                <name>Adwaita</name>
                <font place="ActiveWindow"><name>Inter</name><size>10</size></font>
                <font place="InactiveWindow"><name>Inter</name><size>10</size></font>
            </theme>
            <keyboard>
                <keybind key="W-Return"><action name="Execute"><command>${pkgs.gnome-terminal}/bin/gnome-terminal</command></action></keybind>
                <keybind key="W-q"><action name="Close"/></keybind>
            </keyboard>
            <mouse>
                <context name="TitleBar">
                    <mousebind button="Left" action="Drag"><action name="Move"/></mousebind>
                    <mousebind button="Left" action="DoubleClick"><action name="ToggleMaximize"/></mousebind>
                </context>
                <context name="Frame">
                    <mousebind button="W-Left" action="Drag"><action name="Move"/></mousebind>
                    <mousebind button="W-Right" action="Drag"><action name="Resize"/></mousebind>
                </context>
            </mouse>
        </labwc_config>
    '';

    labwcEnv = pkgs.writeText "labwc-environment" ''
        XDG_CURRENT_DESKTOP=GNOME
        XDG_DATA_DIRS=/run/current-system/sw/share
        FONTCONFIG_FILE=/etc/fonts/fonts.conf
        MOZ_ENABLE_WAYLAND=1
        GTK_THEME=adw-gtk3-dark
        XCURSOR_THEME=Adwaita
        XCURSOR_SIZE=24
        ADW_DISABLE_PORTAL=1
    '';

    labwcAutostart = pkgs.writeShellScript "labwc-autostart" ''
        # Dark background
        ${pkgs.swaybg}/bin/swaybg -c '#1a1a2e' &

        # Write GTK font + theme directly to dconf (no schema lookup needed)
        export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u)/bus"
        ${pkgs.dconf}/bin/dconf write /org/gnome/desktop/interface/font-name "'Inter 11'"
        ${pkgs.dconf}/bin/dconf write /org/gnome/desktop/interface/monospace-font-name "'JetBrains Mono 11'"
        ${pkgs.dconf}/bin/dconf write /org/gnome/desktop/interface/color-scheme "'prefer-dark'"
        ${pkgs.dconf}/bin/dconf write /org/gnome/desktop/interface/icon-theme "'Adwaita'"
        ${pkgs.dconf}/bin/dconf write /org/gnome/desktop/interface/gtk-theme "'adw-gtk3-dark'"

        # Set gnome-terminal profile font
        ${pkgs.dconf}/bin/dconf write /org/gnome/terminal/legacy/profiles:/:b1dcc9dd-5262-4d8d-a863-c897e6d979b9/use-system-font false
        ${pkgs.dconf}/bin/dconf write /org/gnome/terminal/legacy/profiles:/:b1dcc9dd-5262-4d8d-a863-c897e6d979b9/font "'JetBrains Mono 11'"

        # Debug log
        ${pkgs.dconf}/bin/dconf read /org/gnome/desktop/interface/font-name > /tmp/font-debug.log 2>&1
        ${pkgs.dconf}/bin/dconf read /org/gnome/desktop/interface/monospace-font-name >> /tmp/font-debug.log 2>&1
        echo "---" >> /tmp/font-debug.log
        ${pkgs.fontconfig}/bin/fc-match sans-serif >> /tmp/font-debug.log 2>&1
        ${pkgs.fontconfig}/bin/fc-match monospace >> /tmp/font-debug.log 2>&1

        # Bottom taskbar
        ${pkgs.waybar}/bin/waybar &

        # Launch installer
        sleep 1
        ${bingux-installer}/bin/bingux-installer &
    '';

    waybarConfig = pkgs.writeText "waybar-config" (builtins.toJSON {
        layer = "top";
        position = "bottom";
        height = 36;
        modules-left = [ "wlr/taskbar" ];
        modules-right = [ "clock" ];
        "wlr/taskbar" = {
            format = "{icon} {title}";
            on-click = "activate";
            icon-size = 20;
        };
        clock = {
            format = "{:%H:%M}";
        };
    });

    waybarStyle = pkgs.writeText "waybar-style.css" ''
        * {
            font-family: Inter, sans-serif;
            font-size: 13px;
            color: #ffffff;
        }
        window#waybar {
            background: rgba(26, 26, 46, 0.9);
            border-top: 1px solid rgba(255, 255, 255, 0.1);
        }
        #taskbar button {
            padding: 2px 8px;
            border-radius: 6px;
            margin: 2px 2px;
        }
        #taskbar button.active {
            background: rgba(82, 119, 195, 0.6);
        }
        #clock {
            padding: 0 12px;
        }
    '';

    labwcSession = pkgs.writeShellScript "labwc-session" ''
        export XDG_CONFIG_HOME="$HOME/.config"
        mkdir -p "$XDG_CONFIG_HOME/labwc" "$XDG_CONFIG_HOME/waybar"
        ln -sf ${labwcRc} "$XDG_CONFIG_HOME/labwc/rc.xml"
        ln -sf ${labwcEnv} "$XDG_CONFIG_HOME/labwc/environment"
        ln -sf ${labwcAutostart} "$XDG_CONFIG_HOME/labwc/autostart"
        ln -sf ${waybarConfig} "$XDG_CONFIG_HOME/waybar/config"
        ln -sf ${waybarStyle} "$XDG_CONFIG_HOME/waybar/style.css"
        exec ${pkgs.labwc}/bin/labwc
    '';
in
{
    imports = [
        (modulesPath + "/installer/cd-dvd/installation-cd-minimal.nix")
        ./live-shell.nix
        ../system/branding.nix
    ];

    # greetd auto-login into labwc
    services.greetd = {
        enable = true;
        settings.default_session = {
            command = "${labwcSession}";
            user = "bingux";
        };
    };

    # Passwordless bingux user
    users.users.bingux.hashedPassword = lib.mkForce "";
    security.sudo.wheelNeedsPassword = lib.mkForce false;

    # Polkit for privileged operations
    security.polkit.enable = true;
    security.polkit.extraConfig = ''
        polkit.addRule(function(action, subject) {
            if (subject.user == "bingux") {
                return polkit.Result.YES;
            }
        });
    '';

    # Wayland essentials
    xdg.portal = {
        enable = true;
        wlr.enable = true;
        extraPortals = [ pkgs.xdg-desktop-portal-gtk ];
    };
    programs.dconf.enable = true;

    # Ensure GSettings schemas are compiled and available
    environment.pathsToLink = [ "/share/glib-2.0" ];
    environment.extraInit = ''
        export XDG_DATA_DIRS="/run/current-system/sw/share:$HOME/.local/share:$XDG_DATA_DIRS"
        export GSETTINGS_SCHEMA_DIR="/run/current-system/sw/share/glib-2.0/schemas"
    '';
    programs.dconf.profiles.user.databases = [{
        settings."org/gnome/desktop/interface" = {
            font-name = "Inter 11";
            monospace-font-name = "JetBrains Mono 11";
            color-scheme = "prefer-dark";
            icon-theme = "Adwaita";
            gtk-theme = "adw-gtk3-dark";
        };
    }];

    # Suppress zsh new-user setup prompt
    system.activationScripts.zshrc.text = ''
        echo "# Bingux installer" > /home/bingux/.zshrc 2>/dev/null || true
    '';

    # GTK settings (dark theme, Inter font, Adwaita icons)
    environment.etc."xdg/gtk-4.0/settings.ini".text = ''
        [Settings]
        gtk-font-name=Inter 11
        gtk-icon-theme-name=Adwaita
        gtk-theme-name=adw-gtk3-dark
        gtk-application-prefer-dark-theme=true
    '';
    environment.etc."xdg/gtk-3.0/settings.ini".text = ''
        [Settings]
        gtk-font-name=Inter 11
        gtk-icon-theme-name=Adwaita
        gtk-theme-name=adw-gtk3-dark
        gtk-application-prefer-dark-theme=true
    '';

    environment.defaultPackages = lib.mkForce (with pkgs; [
        vim
        nano
    ]);

    environment.systemPackages = with pkgs; [
        # Installer
        bingux-installer

        # Compositor + panel
        labwc
        waybar
        swaybg
        wl-clipboard

        # Browser + tools
        firefox
        gh
        gparted
        gptfdisk
        ssh-to-age
        gnome-terminal
        gnome-text-editor

        # Icons + cursors + theme + schemas
        adwaita-icon-theme
        hicolor-icon-theme
        adw-gtk3
        gsettings-desktop-schemas
        glib
    ];

    # Locale
    i18n.defaultLocale = lib.mkForce "en_US.UTF-8";
    i18n.supportedLocales = lib.mkForce [
        "en_US.UTF-8/UTF-8"
        "en_GB.UTF-8/UTF-8"
    ];

    # Trim
    virtualisation.vmware.guest.enable = lib.mkForce false;
    programs.nix-index.enable = lib.mkForce false;
    programs.direnv.enable = lib.mkForce false;
    documentation.enable = lib.mkForce false;
    documentation.man.enable = lib.mkForce false;
    documentation.info.enable = lib.mkForce false;
    documentation.doc.enable = lib.mkForce false;
    documentation.nixos.enable = lib.mkForce false;
    hardware.enableAllFirmware = lib.mkForce false;
    hardware.enableRedistributableFirmware = true;
    services.printing.enable = lib.mkForce false;
    hardware.bluetooth.enable = lib.mkForce false;
    services.power-profiles-daemon.enable = lib.mkForce false;
    services.speechd.enable = lib.mkForce false;
    boot.swraid.enable = lib.mkForce false;
    virtualisation.hypervGuest.enable = lib.mkForce false;

    # Virtio drivers
    boot.initrd.availableKernelModules = [
        "virtio_pci" "virtio_blk" "virtio_scsi" "virtio_net"
        "ahci" "sd_mod" "sr_mod" "usb_storage" "ehci_pci"
        "uhci_hcd" "xhci_pci" "nvme" "ata_piix"
    ];

    boot.supportedFilesystems = lib.mkForce [ "btrfs" "ext4" "xfs" "f2fs" "vfat" "ntfs3" ];

    nix.gc.automatic = true;
    nix.settings.auto-optimise-store = true;
    nix.settings.max-jobs = "auto";
    nix.settings.cores = 0;

    # Fonts
    fonts.packages = with pkgs; [ adwaita-fonts inter jetbrains-mono ];
    fonts.fontconfig.defaultFonts = {
        sansSerif = lib.mkForce [ "Inter" ];
        monospace = lib.mkForce [ "JetBrains Mono" ];
    };

    # Plymouth + quiet boot
    boot.plymouth = {
        enable = lib.mkForce true;
        theme = lib.mkForce "bingux";
        themePackages = lib.mkForce [ bingux-plymouth ];
    };
    boot.kernelParams = [ "quiet" "splash" "loglevel=3" "rd.systemd.show_status=false" "udev.log_priority=3" ];
    boot.consoleLogLevel = 0;
    boot.initrd.verbose = false;

    nix.settings.experimental-features = [ "nix-command" "flakes" ];
    system.nixos.distroName = "Bingux";
    isoImage.squashfsCompression = "zstd -Xcompression-level 22";
    isoImage.efiSplashImage = ../../files/branding/bingus.png;
    isoImage.splashImage = ../../files/branding/bingus-syslinux.png;

    # GRUB theme
    isoImage.grubTheme = let
        base = pkgs.nixos-grub2-theme;
    in pkgs.runCommand "bingux-grub-theme" {} ''
        cp -r ${base} $out
        chmod -R u+w $out
        for f in $out/icons/nixos.png $out/logo.png; do
            if [ -f "$f" ]; then
                ${pkgs.imagemagick}/bin/magick -size 1x1 xc:#1a1a2e -depth 8 -type TrueColor \
                    PNG24:"$f" 2>/dev/null || true
            fi
        done
        if [ -f "$out/background.png" ]; then
            ${pkgs.imagemagick}/bin/magick -size 1920x1080 xc:#1a1a2e -depth 8 -type TrueColor \
                PNG24:"$out/background.png" 2>/dev/null || true
        fi
        if [ -f "$out/theme.txt" ]; then
            sed -i 's/NixOS/Bingux/g' "$out/theme.txt"
        fi
        ${pkgs.grub2}/bin/grub-mkfont \
            -s 24 -o "$out/font.pf2" \
            ${pkgs.jetbrains-mono}/share/fonts/truetype/JetBrainsMono-Regular.ttf 2>/dev/null || true
    '';

    isoImage.syslinuxTheme = ''
        MENU TITLE Bingux
        MENU RESOLUTION 800 600
        MENU CLEAR
        MENU ROWS 6
        MENU CMDLINEROW -4
        MENU TIMEOUTROW -3
        MENU TABMSGROW  -2
        MENU HELPMSGROW -1
        MENU HELPMSGENDROW -1
        MENU MARGIN 0

        MENU COLOR BORDER       30;44      #00000000    #00000000   none
        MENU COLOR SCREEN       37;40      #FF000000    #00E2E8FF   none
        MENU COLOR TABMSG       31;40      #80000000    #00000000   none
        MENU COLOR TIMEOUT      1;37;40    #FF000000    #00000000   none
        MENU COLOR TIMEOUT_MSG  37;40      #FF000000    #00000000   none
        MENU COLOR CMDMARK      1;36;40    #FF000000    #00000000   none
        MENU COLOR CMDLINE      37;40      #FF000000    #00000000   none
        MENU COLOR TITLE        1;36;44    #00000000    #00000000   none
        MENU COLOR UNSEL        37;44      #FF000000    #00000000   none
        MENU COLOR SEL          7;37;40    #FFFFFFFF    #FF5277C3   std
    '';

    networking.hostName = "bingux-installer";

    # Autostart desktop entry for app launchers
    environment.etc."xdg/autostart/bingux-installer.desktop".text = ''
        [Desktop Entry]
        Type=Application
        Name=Install Bingux
        Exec=${bingux-installer}/bin/bingux-installer
        Icon=bingux
        Categories=System;
        Terminal=false
    '';
}
