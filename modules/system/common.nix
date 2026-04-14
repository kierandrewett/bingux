{ config, lib, pkgs, ... }:
{
    imports = [
        ./monitors.nix
        ./nix.nix
        ./boot.nix
        ./branding.nix
        ./audio.nix
        ./desktop
        ./fonts.nix
        ./gpg.nix
        ./locale.nix
    ];

    config = {
        services.automatic-timezoned.enable = lib.mkDefault true;
        services.openssh.enable = lib.mkDefault false;
        services.geoclue2.enable = lib.mkDefault true;
        services.earlyoom.enable = lib.mkDefault true;
        location.provider = lib.mkDefault "geoclue2";

        # Sync DE location toggle with system geoclue setting
        programs.dconf.profiles.user.databases = lib.mkIf config.services.geoclue2.enable [{
            settings."org/gnome/system/location" = {
                enabled = true;
            };
        }];

        hardware.bluetooth.enable = lib.mkDefault true;
        hardware.bluetooth.powerOnBoot = lib.mkDefault true;

        # Printing
        services.printing.enable = lib.mkDefault true;
        services.printing.drivers = [ pkgs.gutenprint pkgs.hplip ];

        # mDNS / Bonjour
        services.avahi = {
            enable = lib.mkDefault true;
            nssmdns4 = lib.mkDefault true;
            openFirewall = lib.mkDefault true;
            publish = {
                enable = lib.mkDefault true;
                addresses = lib.mkDefault true;
                workstation = lib.mkDefault true;
            };
        };

        # Default shell
        programs.zsh.enable = lib.mkDefault true;
        programs.zsh.interactiveShellInit = lib.mkDefault ''
            # Suppress zsh-newuser-install wizard
            zsh-newuser-install() { :; }
        '';
        programs.zsh.promptInit = lib.mkDefault ''
            PS1='%F{blue}%n%f@%F{magenta}%m%f:%F{cyan}%~%f > '
        '';

        # Create .zshrc for new users so the wizard never triggers
        environment.etc."skel/.zshrc".text = "# Bingux\n";

        # Bingux CLI helper + essentials
        environment.systemPackages = with pkgs; [ os-helper bingux-cli fastfetch-wrapped desktop-file-utils ];

        # bgx profiles — volatile (/tmp, cleared on reboot) + permanent (persists)
        environment.extraInit = ''
            for profile in "/tmp/bgx-session-$USER-packages" "/nix/var/nix/profiles/per-user/$USER/bgx/packages"; do
                if [ -d "$profile/bin" ]; then
                    export PATH="$profile/bin:$PATH"
                fi
                if [ -d "$profile/share" ]; then
                    export XDG_DATA_DIRS="$profile/share:$XDG_DATA_DIRS"
                fi
            done
        '';

        # Mark /os as safe for git
        environment.etc."gitconfig".text = lib.mkDefault ''
            [safe]
                directory = /os
        '';
    };
}
