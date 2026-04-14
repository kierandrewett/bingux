{ config, lib, pkgs, ... }:
let
    cfg = config.bingux.desktop;
    isGnomeBase = cfg == "gnome" || cfg == "gnome-default";
    isGnomeFull = cfg == "gnome";
    gv = lib.gvariant;

    # Sync rounded-window-corners border color with GNOME light/dark theme
    themeSyncScript = pkgs.writeShellScript "bingux-theme-sync" ''
        export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$(id -u)/bus"

        DARK_VALUE="{'padding': <{'left': <uint32 1>, 'right': <uint32 1>, 'top': <uint32 1>, 'bottom': <uint32 1>}>, 'keepRoundedCorners': <{'maximized': <false>, 'fullscreen': <false>}>, 'borderRadius': <uint32 12>, 'smoothing': <0.29999999999999999>, 'borderColor': <(0.24333333969116211, 0.24333333969116211, 0.24333333969116211, 0.34000000357627869)>, 'enabled': <true>}"
        LIGHT_VALUE="{'padding': <{'left': <uint32 1>, 'right': <uint32 1>, 'top': <uint32 1>, 'bottom': <uint32 1>}>, 'keepRoundedCorners': <{'maximized': <false>, 'fullscreen': <false>}>, 'borderRadius': <uint32 12>, 'smoothing': <0.29999999999999999>, 'borderColor': <(0.64333333158493042, 0.64333333158493042, 0.64333333158493042, 0.53666669130325317)>, 'enabled': <true>}"

        DCONF="${pkgs.dconf}/bin/dconf"
        KEY="/org/gnome/shell/extensions/rounded-window-corners-reborn/global-rounded-corner-settings"

        set_corners() {
            $DCONF write "$KEY" "$1" 2>/dev/null || true
        }

        # Set initial value
        scheme=$($DCONF read /org/gnome/desktop/interface/color-scheme 2>/dev/null)
        if [[ "$scheme" == *"dark"* ]]; then
            set_corners "$DARK_VALUE"
        else
            set_corners "$LIGHT_VALUE"
        fi

        # Monitor for changes
        $DCONF watch /org/gnome/desktop/interface/color-scheme | while read -r line; do
            if [[ "$line" == *"dark"* ]]; then
                set_corners "$DARK_VALUE"
            elif [[ "$line" == *"light"* ]] || [[ "$line" == *"default"* ]]; then
                set_corners "$LIGHT_VALUE"
            fi
        done
    '';
in
{
    # ── Base GNOME (shared by "gnome" and "gnome-default") ──
    config = lib.mkIf isGnomeBase (lib.mkMerge [
        {
            security.pam.services.gdm-password.enableGnomeKeyring = lib.mkDefault true;
            services.gnome.gnome-keyring.enable = lib.mkDefault true;

            services.xserver = {
                enable = lib.mkDefault true;
                displayManager.gdm.enable = lib.mkDefault true;
                desktopManager.gnome.enable = lib.mkDefault true;
            };

            services.displayManager.defaultSession = lib.mkDefault "gnome";

            # Remove distro logo from GDM login screen
            programs.dconf.profiles.gdm.databases = [{
                settings."org/gnome/login-screen" = {
                    logo = "";
                };
            }];

            # Strip default GNOME bloat
            environment.gnome.excludePackages = with pkgs; [
                epiphany
                geary
                gnome-tour
                yelp
                gnome-music
                gnome-photos
                gnome-software
                nixos-backgrounds
            ];

            environment.systemPackages = with pkgs; [
                gnome-extension-manager
                gnomeExtensions.user-themes
                gnomeExtensions.appindicator
                gnome-calculator
                gnome-backgrounds
                gnome-text-editor
                snapshot
                gnome-font-viewer
                evince
                loupe
                gnome-characters
                gnome-tweaks
                adw-gtk3
            ];
        }

        # ── Full Bingux GNOME (extensions + dconf defaults + theme-sync) ──
        (lib.mkIf isGnomeFull {
            environment.systemPackages = with pkgs; [
                gnomeExtensions.blur-my-shell
                gnomeExtensions.dash-to-dock
                gnomeExtensions.grand-theft-focus
                gnomeExtensions.rounded-window-corners-reborn
                gnomeExtensions.night-theme-switcher
            ];

            programs.dconf.profiles.user.databases = [{
                settings = {
                    "org/gnome/shell" = {
                        enabled-extensions = [
                            "appindicatorsupport@rgcjonas.gmail.com"
                            "blur-my-shell@aunetx"
                            "dash-to-dock@micxgx.gmail.com"
                            "grand-theft-focus@zalckos.github.com"
                            "rounded-window-corners@fxgn"
                            "user-theme@gnome-shell-extensions.gcampax.github.com"
                            "nightthemeswitcher@romainvigier.fr"
                        ];
                    };

                    "org/gnome/shell/extensions/dash-to-dock" = {
                        dock-position = "BOTTOM";
                        dock-fixed = true;
                        dash-max-icon-size = gv.mkInt32 56;
                        click-action = "minimize";
                        background-opacity = gv.mkDouble 0.0;
                        transparency-mode = "FIXED";
                        disable-overview-on-startup = true;
                        show-mounts = false;
                        show-trash = false;
                        hide-tooltip = false;
                        running-indicator-style = "DOTS";
                    };

                    "org/gnome/shell/extensions/blur-my-shell" = {
                        sigma = gv.mkInt32 30;
                        brightness = gv.mkDouble 0.6;
                    };
                    "org/gnome/shell/extensions/blur-my-shell/dash-to-dock" = {
                        blur = true;
                        static-blur = true;
                        brightness = gv.mkDouble 0.6;
                    };
                    "org/gnome/shell/extensions/blur-my-shell/panel" = {
                        sigma = gv.mkInt32 13;
                        static-blur = true;
                        brightness = gv.mkDouble 0.5;
                        override-background = true;
                    };
                    "org/gnome/shell/extensions/blur-my-shell/applications" = {
                        blur = true;
                    };

                    "org/gnome/shell/extensions/rounded-window-corners-reborn" = {
                        corner-radius = gv.mkInt32 12;
                        corner-smoothing = gv.mkDouble 0.3;
                        border-width = gv.mkInt32 (-1);
                        keep-rounded-corners-maximized = false;
                        keep-rounded-corners-fullscreen = false;
                        skip-libadwaita-app = false;
                        tweak-kitty-terminal = false;
                    };

                    # Match GNOME location toggle to system geoclue setting
                    "org/gnome/system/location" = {
                        enabled = config.services.geoclue2.enable;
                    };

                    # adw-gtk3 for legacy GTK3 apps
                    "org/gnome/desktop/interface" = {
                        gtk-theme = "adw-gtk3-dark";
                        color-scheme = "prefer-dark";
                    };


                    "org/gnome/shell/extensions/nightthemeswitcher/time" = {
                        manual-schedule = false;
                    };
                };
            }];

            systemd.user.services.bingux-theme-sync = {
                description = "Sync rounded corners border color with GNOME theme";
                after = [ "graphical-session.target" ];
                partOf = [ "graphical-session.target" ];
                wantedBy = [ "graphical-session.target" ];
                serviceConfig = {
                    ExecStart = "${themeSyncScript}";
                    Restart = "on-failure";
                    RestartSec = 5;
                    # Delay start to let extensions load
                    ExecStartPre = "${pkgs.coreutils}/bin/sleep 5";
                };
            };
        })
    ]);
}
