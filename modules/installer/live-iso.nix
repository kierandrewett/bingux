{ lib, modulesPath, pkgs, ... }:
let
    bingux-plymouth = pkgs.callPackage ../../pkgs/bingux-plymouth { };
    bingux-installer = pkgs.callPackage ../../pkgs/bingux-installer { };

    # Minimal GNOME Shell theme that hides the top bar
    installerShellTheme = pkgs.runCommand "bingux-installer-shell-theme" {} ''
        mkdir -p $out/share/themes/BinguxInstaller/gnome-shell
        cat > $out/share/themes/BinguxInstaller/gnome-shell/gnome-shell.css << 'CSS'
        @import url("resource:///org/gnome/shell/theme/gnome-shell.css");
        #panel { height: 0; opacity: 0; pointer-events: none; }
        CSS
    '';
in
{
    imports = [
        (modulesPath + "/installer/cd-dvd/installation-cd-graphical-base.nix")
        ./live-shell.nix
        ../system/branding.nix
    ];

    # GNOME + GDM autologin
    services.xserver.desktopManager.gnome.enable = lib.mkForce true;
    services.xserver.displayManager.gdm.enable = lib.mkForce true;
    services.displayManager.defaultSession = "gnome";
    services.displayManager.autoLogin = {
        enable = true;
        user = "bingux";
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

    # Dark theme, no wallpaper, installer in dock
    programs.dconf.profiles.user.databases = [{
        settings."org/gnome/desktop/background" = {
            picture-options = "none";
            primary-color = "#1a1a2e";
        };
        settings."org/gnome/desktop/screensaver" = {
            picture-options = "none";
            primary-color = "#1a1a2e";
        };
        settings."org/gnome/desktop/interface" = {
            color-scheme = "prefer-dark";
            font-name = "Inter 11";
            monospace-font-name = "JetBrains Mono 11";
        };
        settings."org/gnome/shell" = {
            enabled-extensions = [
                "user-theme@gnome-shell-extensions.gcampax.github.com"
                "dash-to-dock@micxgx.gmail.com"
            ];
            favorite-apps = [
                "dev.drewett.BinguxInstaller.desktop"
                "org.gnome.Nautilus.desktop"
                "org.gnome.Terminal.desktop"
                "firefox.desktop"
            ];
        };
    }];

        settings."org/gnome/shell/extensions/user-theme" = {
            name = "BinguxInstaller";
        };
        settings."org/gnome/shell/extensions/dash-to-dock" = {
            dock-position = "BOTTOM";
            dock-fixed = true;
            dash-max-icon-size = lib.gvariant.mkInt32 48;
            background-opacity = lib.gvariant.mkDouble 0.0;
            transparency-mode = "FIXED";
            disable-overview-on-startup = true;
            show-mounts = false;
            show-trash = false;
        };

    # Remove distro logo from GDM
    programs.dconf.profiles.gdm.databases = [{
        settings."org/gnome/login-screen" = {
            logo = "";
        };
    }];

    environment.defaultPackages = lib.mkForce (with pkgs; [
        vim
        nano
    ]);

    environment.systemPackages = with pkgs; [
        # Installer
        bingux-installer

        # Browser + tools
        firefox
        gh
        gparted
        gptfdisk
        ssh-to-age

        # GNOME apps (minimal)
        gnome-terminal
        gnome-text-editor

        # Icons + theme
        adwaita-icon-theme
        hicolor-icon-theme
        installerShellTheme
        gnomeExtensions.user-themes
        gnomeExtensions.dash-to-dock
    ];

    # Locale
    i18n.defaultLocale = lib.mkForce "en_US.UTF-8";
    i18n.supportedLocales = lib.mkForce [
        "en_US.UTF-8/UTF-8"
        "en_GB.UTF-8/UTF-8"
    ];

    # Suppress zsh new-user setup prompt
    environment.etc."skel/.zshrc".text = "# Bingux installer\n";

    # Strip GNOME bloat
    environment.gnome.excludePackages = with pkgs; [
        epiphany geary gnome-music gnome-photos gnome-software gnome-tour
        yelp gnome-maps gnome-contacts gnome-weather gnome-clocks
        gnome-calendar gnome-characters gnome-connections gnome-console
        gnome-logs gnome-system-monitor baobab simple-scan totem evince
        snapshot gnome-font-viewer gnome-disk-utility
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

    # GRUB theme — dark background, no NixOS logo
    isoImage.grubTheme = let
        base = pkgs.nixos-grub2-theme;
    in pkgs.runCommand "bingux-grub-theme" {} ''
        cp -r ${base} $out
        chmod -R u+w $out
        # Remove NixOS logo
        for f in $out/icons/nixos.png $out/logo.png; do
            if [ -f "$f" ]; then
                ${pkgs.imagemagick}/bin/magick -size 1x1 xc:#1a1a2e -depth 8 -type TrueColor \
                    PNG24:"$f" 2>/dev/null || true
            fi
        done
        # Dark background
        if [ -f "$out/background.png" ]; then
            ${pkgs.imagemagick}/bin/magick -size 1920x1080 xc:#1a1a2e -depth 8 -type TrueColor \
                PNG24:"$out/background.png" 2>/dev/null || true
        fi
        # Rebrand text
        if [ -f "$out/theme.txt" ]; then
            sed -i 's/NixOS/Bingux/g' "$out/theme.txt"
        fi
        # JetBrains Mono font for GRUB
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

    # Autostart the installer
    environment.etc."xdg/autostart/bingux-installer.desktop".text = ''
        [Desktop Entry]
        Type=Application
        Name=Install Bingux
        Comment=Install Bingux to your disk
        Exec=${bingux-installer}/bin/bingux-installer
        Icon=bingux
        Terminal=false
        Categories=System;
        X-GNOME-Autostart-Phase=Application
        X-GNOME-Autostart-Delay=2
    '';
}
