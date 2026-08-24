{
    config,
    inputs,
    pkgs,
    ...
}:
let
    user = config.bingux.user;
    hmGvariant = inputs.home-manager.lib.hm.gvariant;
in
{
    bingux = {
        profile.name = "kieran";

        system = {
            timeZone = "Europe/London";
            locale = "en_GB.UTF-8";
            keyMap = "uk";
        };

        user = {
            name = "kieran";
            fullName = "Kieran Drewett";
        };

        packages = {
            system = with pkgs; [
                # Declarative Rust, C++, and TypeScript toolchains.
                rustc
                cargo
                rustfmt
                clippy
                cargo-audit
                cargo-generate
                cargo-llvm-cov
                sqlx-cli
                trunk
                sccache
                gcc
                clang
                clang-tools
                ccache
                gdb
                mold
                cmake
                ninja
                gnumake
                pkg-config
                nodejs
                typescript
                pnpm
                bun
                esbuild
                prettier
                sass

                # Rootless container tooling.
                podman
                podman-compose
                buildah
                skopeo

                # Source control, terminals, and everyday workstation tooling.
                git
                git-lfs
                gh
                curl
                jq
                ripgrep
                fd
                fzf
                bat
                eza
                tree
                file
                unzip
                zip
                direnv
                tmux
                just
                zellij
                fastfetch
                lsof
                mosh
                btop
                zoxide
                age
                sops
                wl-clipboard
            ];

            user = with pkgs; [
                # Profile-owned graphical workstation applications.
                firefox
                chromium
                vscodium
                thunderbird
                libreoffice
                keepassxc
                vlc
                pavucontrol
                evince
                gnome-calculator
                gnome-disk-utility
            ];

            flatpaks = import ./flatpaks.nix;
        };

        # Enabling this installs bingux-secrets-init. Add encrypted entries only after
        # an age key exists and the profile has a SOPS recipient.
        secrets.enable = true;

        networking.tailscale.enable = true;

        desktop = {
            enable = true;
            gnoblin.enable = true;
        };

        desktopShell.enable = true;

        performance = {
            enable = true;
            kernel = "cachyos-bore-lto-x86_64-v3";
            cpuGovernor = "performance";
            enableAmdPstate = true;
        };
    };

    services.displayManager = {
        gdm.enable = true;
        defaultSession = "gnoblin";
    };

    home-manager.users.${user.name} = {
        home = {
            username = user.name;
            homeDirectory = user.home;
            stateVersion = config.bingux.system.stateVersion;
        };

        programs.home-manager.enable = true;
        dconf.settings = {
            "org/gnome/desktop/input-sources" = {
                sources = [
                    (hmGvariant.mkTuple [
                        "xkb"
                        "gb"
                    ])
                    (hmGvariant.mkTuple [
                        "xkb"
                        "us"
                    ])
                ];
            };

            "org/gnome/desktop/wm/keybindings" = {
                switch-input-source = [ "<Super>space" ];
                switch-input-source-backward = [ "<Shift><Super>space" ];
            };
        };
    };
}
