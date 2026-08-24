{
    inputs,
    self,
    system,
}:
let
    pkgs = inputs.nixpkgs.legacyPackages.${system};
    hostFactory = import ../lib/mk-host.nix { inherit inputs self; };
    host = hostFactory.mkHost {
        inherit system;
        hostName = "kieran-profile-check";
        profile = "kieran";
    };
    inputSources =
        host.config.home-manager.users.kieran.dconf.settings."org/gnome/desktop/input-sources".sources;
    flatpakRemoteNames = builtins.map (
        remote: remote.name
    ) host.config.bingux.packages.flatpaks.remotes;
    flatpakApps = host.config.bingux.packages.flatpaks.apps;
    tailscaleSystray =
        host.config.home-manager.users.kieran.systemd.user.services.bingux-tailscale-systray;
in
assert host.config.bingux.profile.name == "kieran";
assert host.config.bingux.desktop.enable;
assert host.config.bingux.desktop.gnoblin.enable;
assert host.config.bingux.desktopShell.enable;
assert host.config.bingux.secrets.enable;
assert host.config.programs.gnoblin.enable;
assert host.config.services.displayManager.defaultSession == "gnoblin";
assert host.config.bingux.networking.tailscale.enable;
assert host.config.services.tailscale.enable;
assert host.config.programs.dconf.enable;
assert inputSources.type == "a(ss)";
assert
    builtins.map (source: builtins.map (entry: entry.value) source.value) inputSources.value == [
        [
            "xkb"
            "gb"
        ]
        [
            "xkb"
            "us"
        ]
    ];
assert
    host.config.home-manager.users.kieran.dconf.settings."org/gnome/desktop/wm/keybindings".switch-input-source.type
    == "as";
assert
    builtins.map (entry: entry.value)
        host.config.home-manager.users.kieran.dconf.settings."org/gnome/desktop/wm/keybindings".switch-input-source.value
    == [ "<Super>space" ];
assert
    host.config.home-manager.users.kieran.dconf.settings."org/gnome/desktop/wm/keybindings".switch-input-source-backward.type
    == "as";
assert
    builtins.map (entry: entry.value)
        host.config.home-manager.users.kieran.dconf.settings."org/gnome/desktop/wm/keybindings".switch-input-source-backward.value
    == [ "<Shift><Super>space" ];
assert host.config.home-manager.users.kieran.programs.quickshell.enable;
assert builtins.any (remote: remote.name == "flathub-beta") host.config.services.flatpak.remotes;
assert builtins.elem pkgs.rustc host.config.bingux.packages.system;
assert builtins.elem pkgs.cargo-audit host.config.bingux.packages.system;
assert builtins.elem pkgs.cargo-generate host.config.bingux.packages.system;
assert builtins.elem pkgs.cargo-llvm-cov host.config.bingux.packages.system;
assert builtins.elem pkgs.sqlx-cli host.config.bingux.packages.system;
assert builtins.elem pkgs.ccache host.config.bingux.packages.system;
assert builtins.elem pkgs.gdb host.config.bingux.packages.system;
assert builtins.elem pkgs.bun host.config.bingux.packages.system;
assert builtins.elem pkgs.prettier host.config.bingux.packages.system;
assert builtins.elem pkgs.zellij host.config.bingux.packages.system;
assert builtins.elem pkgs.vscodium host.config.bingux.packages.user;
assert tailscaleSystray.Service.ExecStart == [ "${pkgs.tailscale}/bin/tailscale systray" ];
assert tailscaleSystray.Service.NoNewPrivileges;
assert !(tailscaleSystray.Service ? PrivateTmp);
assert tailscaleSystray.Install.WantedBy == [ "graphical-session.target" ];
assert builtins.all (app: builtins.elem app.origin flatpakRemoteNames) flatpakApps;
assert host.config.services.flatpak.enable;
assert builtins.any (
    app: app.appId == "md.obsidian.Obsidian"
) host.config.bingux.packages.flatpaks.apps;
pkgs.runCommand "bingux-kieran-profile-check" { } ''
    touch "$out"
''
