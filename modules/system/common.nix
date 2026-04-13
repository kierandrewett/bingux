{ lib, pkgs, ... }:
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

        # Bingux CLI helper + essentials
        environment.systemPackages = with pkgs; [ os-helper bingux-cli fastfetch comma ];

        # bgx profiles — volatile (/tmp, cleared on reboot) + permanent (persists)
        environment.extraInit = ''
            for profile in "/tmp/bgx-session-$USER-packages" "/nix/var/nix/profiles/per-user/$USER/bgx/packages"; do
                # Ensure dirs exist so GNOME watches them from login
                mkdir -p "$profile/bin" "$profile/share/applications" 2>/dev/null || true
                export PATH="$profile/bin:$PATH"
                export XDG_DATA_DIRS="$profile/share:$XDG_DATA_DIRS"
            done
        '';

        # Mark /os as safe for git
        environment.etc."gitconfig".text = lib.mkDefault ''
            [safe]
                directory = /os
        '';
    };
}
