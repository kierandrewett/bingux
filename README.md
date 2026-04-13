# Bingux

A NixOS-based Linux distribution with sensible defaults, GNOME extensions, and a graphical installer.

## Quick Start

### Install from ISO

1. Download or build the installer ISO:
   ```
   nix build github:kierandrewett/bingux#installer-iso
   ```
2. Flash the ISO to a USB drive or boot it in a VM.
3. The graphical installer opens automatically:
   - **Fresh Install** — set up hostname, desktop, and user graphically. The installer generates a minimal flake for you.
   - **From Repository** — point to any NixOS config repository on GitHub. The installer uses `nix flake show` to discover all `nixosConfigurations` in the flake, regardless of repo layout. Any flake that exports `nixosConfigurations` works.

### Use Bingux in your own NixOS config

Add Bingux as a flake input and use `mkBinguxHost` to build your system:

```nix
{
    inputs = {
        bingux.url = "github:kierandrewett/bingux";
        nixpkgs.follows = "bingux/nixpkgs";
    };

    outputs = { bingux, ... }: {
        nixosConfigurations.my-host = bingux.lib.mkBinguxHost {
            hostname = "my-host";
            profile = "workstation";   # workstation | laptop | generic
            extraModules = [
                ./hardware-configuration.nix
                ({ ... }: {
                    users.users.me = {
                        isNormalUser = true;
                        extraGroups = [ "wheel" "networkmanager" ];
                    };
                    users.mutableUsers = true;
                    system.stateVersion = "25.05";
                })
            ];
        };
    };
}
```

Then build and switch:

```
sudo nixos-rebuild switch --flake .#my-host
```

## `mkBinguxHost` Options

| Option | Type | Default | Description |
|---|---|---|---|
| `hostname` | string | required | System hostname |
| `profile` | string | required | `"workstation"`, `"laptop"`, or `"generic"` |
| `system` | string | `"x86_64-linux"` | Target architecture |
| `username` | string | `"user"` | Primary user |
| `extraModules` | list | `[]` | Additional NixOS modules |
| `extraOverlays` | list | `[]` | Additional nixpkgs overlays |
| `specialArgs` | attrs | `{}` | Extra args passed to all modules |

## Configuration Options

### Desktop Environment

```nix
bingux.desktop = "gnome";         # Full Bingux GNOME (default)
bingux.desktop = "gnome-default"; # Stock GNOME without Bingux extensions
bingux.desktop = "kde";           # KDE Plasma 6
bingux.desktop = "xfce";          # XFCE
bingux.desktop = null;            # No desktop (server/headless)
```

**`"gnome"`** includes: dash-to-dock, blur-my-shell, rounded-window-corners, night-theme-switcher, appindicator, user-themes, grand-theft-focus, and a theme-sync service.

### Boot

```nix
bingux.boot.luksUuid = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx";  # Enable LUKS
```

### Hardware Configuration Path

When installing from a repository, the installer needs to know where to place the generated `hardware-configuration.nix`. Declare the path in your flake:

```nix
bingux.hardwareConfigPath = {
    my-desktop = "machines/my-desktop";     # relative to repo root
    my-laptop = "hosts/my-laptop";
};
```

If not set, the installer searches common layouts (`machines/<host>/`, `hosts/<host>/`, etc.) and falls back to leaving it at `/mnt/etc/nixos/`.

### Overriding Defaults

All Bingux defaults use `lib.mkDefault`, so you can override any option by simply setting it in your config:

```nix
i18n.defaultLocale = "en_GB.UTF-8";     # Override default en_US
console.keyMap = "uk";                   # Override default us
boot.kernelPackages = pkgs.linuxPackages_6_12;
services.earlyoom.enable = false;        # Disable earlyoom
```

## What's Included

### System
- Pipewire audio (ALSA, PulseAudio, JACK)
- Bluetooth
- Printing (CUPS, Gutenprint, HPLIP)
- mDNS/Bonjour (Avahi)
- GeoClue + automatic timezone
- EarlyOOM
- GPG agent with SSH support
- Nix flakes enabled, weekly garbage collection

### Boot
- systemd-boot
- Plymouth (Bingux theme)
- Latest kernel
- Btrfs support
- Quiet boot

### Fonts
- Adwaita Sans, Google Sans Code, Inter, JetBrains Mono, Noto, Roboto, Fira, Ubuntu Sans, and more

### Profiles

- **workstation** — Disables sleep/suspend/hibernate
- **laptop** — TLP, thermald, powertop, lid switch handling
- **generic** — No hardware-specific tweaks

## The `os` CLI

Every Bingux system includes the `os` command for managing your NixOS config at `/os`:

```
os rebuild          Rebuild and switch to new config
os apply            Apply config without restarting services
os test             Rebuild and test (no bootloader update)
os update           Update flake inputs and rebuild
os edit             Open /os in your editor
os diff             Show uncommitted changes
os log              Show recent commits
os commit [msg]     Stage and commit changes
os push             Commit and push to remote
os status           Show git status
```

Typical workflow after installation:

```
cd /os
git pull               # Pull latest config changes
os rebuild             # Rebuild and switch
```

## Building the ISO

```
nix build .#installer-iso
```

The ISO is at `result/iso/bingux-*.iso`.

## Project Structure

```
flake.nix                    # Exports: nixosModules, overlays, lib, installer-iso
lib/mkBinguxHost.nix         # Consumer integration function
modules/
  system/
    common.nix               # Aggregate module (audio, boot, fonts, etc.)
    desktop/
      gnome.nix              # GNOME + Bingux extensions
      kde.nix                # KDE Plasma 6
      xfce.nix               # XFCE
  profiles/
    workstation.nix
    laptop.nix
    generic.nix
  installer/
    live-iso.nix             # ISO configuration
    live-shell.nix           # Live environment shell
installer-app/               # GTK4 Python graphical installer
pkgs/                        # Custom packages (plymouth, os-helper, etc.)
files/                       # Branding, fonts, fastfetch config
overlays/                    # Nixpkgs overlays
```

## License

MIT
