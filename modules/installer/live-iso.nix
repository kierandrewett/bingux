{ lib, modulesPath, pkgs, ... }:
let
    bingux-plymouth = pkgs.callPackage ../../pkgs/bingux-plymouth { };
    bingux-installer = pkgs.callPackage ../../pkgs/bingux-installer { };
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

    # Passwordless bingux user on the live installer
    users.users.bingux.hashedPassword = lib.mkForce "";
    security.sudo.wheelNeedsPassword = lib.mkForce false;

    # Polkit rule: allow bingux user to run bingux-installer backend as root
    security.polkit.extraConfig = ''
        polkit.addRule(function(action, subject) {
            if (subject.user == "bingux" &&
                action.id == "org.freedesktop.policykit.exec") {
                return polkit.Result.YES;
            }
        });
    '';

    # Plain dark background instead of NixOS wallpaper
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
        };
        settings."org/gnome/shell" = {
            favorite-apps = [
                "dev.drewett.BinguxInstaller.desktop"
                "org.gnome.Nautilus.desktop"
                "firefox.desktop"
                "org.gnome.Terminal.desktop"
            ];
        };
    }];

    environment.defaultPackages = lib.mkForce (with pkgs; [
        vim
        nano
    ]);

    environment.systemPackages = with pkgs; [
        firefox
        gh
        gparted
        gptfdisk
        ssh-to-age
        gnome-terminal
        gnome-text-editor
        bingux-installer
    ];

    # Locale and keyboard
    i18n.defaultLocale = lib.mkForce "en_US.UTF-8";
    i18n.supportedLocales = lib.mkForce [
        "en_US.UTF-8/UTF-8"
        "en_GB.UTF-8/UTF-8"
    ];

    # Strip GNOME bloat from installer
    environment.gnome.excludePackages = with pkgs; [
        epiphany
        geary
        gnome-music
        gnome-photos
        gnome-software
        gnome-tour
        yelp
        gnome-maps
        gnome-contacts
        gnome-weather
        gnome-clocks
        gnome-calendar
        gnome-characters
        gnome-connections
        gnome-console
        gnome-logs
        gnome-system-monitor
        baobab
        simple-scan
        totem
        evince
        snapshot
        gnome-font-viewer
        gnome-disk-utility
    ];

    # Trim VM guest additions
    virtualisation.vmware.guest.enable = lib.mkForce false;

    # Disable nix-index in installer
    programs.nix-index.enable = lib.mkForce false;

    # Disable direnv in installer
    programs.direnv.enable = lib.mkForce false;

    # Exclude documentation
    documentation.enable = lib.mkForce false;
    documentation.man.enable = lib.mkForce false;
    documentation.info.enable = lib.mkForce false;
    documentation.doc.enable = lib.mkForce false;
    documentation.nixos.enable = lib.mkForce false;

    # Only include firmware for common hardware
    hardware.enableAllFirmware = lib.mkForce false;
    hardware.enableRedistributableFirmware = true;

    # Disable unneeded services
    services.printing.enable = lib.mkForce false;
    hardware.bluetooth.enable = lib.mkForce false;
    services.power-profiles-daemon.enable = lib.mkForce false;
    services.speechd.enable = lib.mkForce false;
    boot.swraid.enable = lib.mkForce false;
    virtualisation.hypervGuest.enable = lib.mkForce false;

    # Ensure virtio drivers are available
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
    fonts.packages = with pkgs; [
        adwaita-fonts
    ];

    # Plymouth
    boot.plymouth = {
        theme = lib.mkForce "bingux";
        themePackages = lib.mkForce [ bingux-plymouth ];
    };

    nix.settings.experimental-features = [ "nix-command" "flakes" ];
    system.nixos.distroName = "Bingux";
    isoImage.squashfsCompression = "zstd -Xcompression-level 22";
    isoImage.efiSplashImage = ../../files/branding/bingus.png;
    isoImage.splashImage = ../../files/branding/bingus-syslinux.png;
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

    # Autostart the GTK4 installer
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

    environment.etc."skel/Desktop/install-bingux.desktop".text = ''
        [Desktop Entry]
        Type=Application
        Name=Install Bingux
        Comment=Install Bingux to your disk
        Exec=${bingux-installer}/bin/bingux-installer
        Icon=bingux
        Terminal=false
        Categories=System;
    '';
}
