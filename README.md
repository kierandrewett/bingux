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

## Package Management (`bgx`)

Every Bingux system includes `bgx`, a package manager that wraps nix with a friendly interface.

### Quick syntax

```
bgx +firefox                    Install for this session (gone after reboot)
bgx ++firefox                   Install permanently (survives reboot)
bgx -firefox                    Remove from this session
bgx --firefox                   Remove permanently
bgx +firefox ++htop -chromium   Batch operations
bgx ?browser                    Search nixpkgs
bgx                             Show help
```

### Subcommands

```
bgx install firefox             Install for this session
bgx install -p firefox          Install permanently
bgx remove firefox              Remove from this session
bgx remove -p firefox           Remove permanently
bgx search firefox              Search nixpkgs
bgx info firefox                Show package details
bgx list                        List installed packages
```

### How it works

bgx manages two nix profiles:

- **Session** (`+` / `-`) — stored in `/tmp`, cleared on reboot. Good for trying things out.
- **Permanent** (`++` / `--` / `-p`) — stored in your nix profile directory, survives reboots. For apps you want to keep.

Installed apps automatically appear in your desktop's app menu (GNOME, KDE, etc.) and are available in PATH across all terminals.

### What stays and what goes after `os rebuild`?

`os rebuild` rebuilds the system from your NixOS config. It does **not** touch your home directory.

**Survives rebuild and reboot:**
- Everything in your home directory (`~/`)
- VS Code extensions, browser bookmarks/extensions, app settings
- bgx permanent packages (`++` / `-p`)
- Any personal files and configs

**Survives reboot but cleared by rebuild:**
- Nothing — bgx permanent packages survive both

**Cleared on reboot:**
- bgx session packages (`+`)

**Rebuilt from config:**
- System packages defined in your NixOS config

Your personal data (home directory, browser profiles, editor extensions, app settings) is always safe. NixOS only manages the system — it never touches `~/`.

### What bgx is for

bgx is for standalone apps and CLI tools — browsers, editors, terminals, dev tools, media players, utilities. These install binaries and `.desktop` files that work immediately.

For things tightly integrated with a host app, use their native mechanisms:
- **VS Code extensions** — `Ctrl+Shift+X` or `code --install-extension`
- **Browser extensions** — install from the browser's extension store
- **GNOME extensions** — managed via system config or extension manager
- **Python/pip packages** — `bgx +python3` then use pip normally
- **Cargo crates** — `bgx +cargo` then use cargo normally

## `mkBinguxHost` Options

| Option | Type | Default | Description |
|---|---|---|---|
| `hostname` | string | required | System hostname |
| `profile` | string | required | `"workstation"`, `"laptop"`, or `"generic"` |
| `system` | string | `"x86_64-linux"` | Target architecture |
| `username` | string | `"user"` | Primary user |
| `hardwareConfigPath` | string | `"machines/<hostname>"` | Where the installer places hardware-configuration.nix |
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

### Locale

```nix
bingux.locale = "en_GB.UTF-8";              # Sets locale + console keymap (uk) automatically
bingux.extraLocales = [ "en_US.UTF-8" ];    # Additional supported locales
```

The keymap is derived from the locale automatically (en_GB -> uk, de_DE -> de, fr_FR -> fr, etc.).

### Fonts

```nix
bingux.fonts.sansSerif = "Inter";           # Used in DE, login screen, Plymouth
bingux.fonts.monospace = "JetBrains Mono";  # Used in terminals and editors
bingux.fonts.serif = "Source Serif";
```

Defaults: Adwaita Sans, Google Sans Code, Noto Serif. The sans-serif font is also used for the Plymouth boot splash.

### Boot

```nix
bingux.boot.luksUuid = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx";  # Enable LUKS
```

### Overriding Defaults

All Bingux defaults use `lib.mkDefault`, so you can override any option by simply setting it:

```nix
boot.kernelPackages = pkgs.linuxPackages_6_12;
services.earlyoom.enable = false;
bingux.desktop = null;    # Strip the desktop entirely
```

## The `os` CLI

Every Bingux system includes the `os` command for managing your NixOS config at `/os`:

```
os rebuild    Rebuild and switch to new config
os test       Rebuild and test (no bootloader update)
os update     Update flake inputs and rebuild
```

## What's Included

### System
- Pipewire audio (ALSA, PulseAudio, JACK)
- Bluetooth, printing (CUPS), mDNS/Bonjour (Avahi)
- GeoClue + automatic timezone
- EarlyOOM, GPG agent with SSH support
- Nix flakes enabled, weekly garbage collection
- zsh as default shell, fastfetch

### Boot
- systemd-boot, Plymouth (Bingux theme), latest kernel, btrfs, quiet boot

### Fonts
- Adwaita Sans, Google Sans Code, Inter, JetBrains Mono, Noto, Roboto, Fira, Ubuntu Sans, and more

### Profiles
- **workstation** — No hardware-specific tweaks (override in your config)
- **laptop** — TLP, thermald, powertop, lid switch handling
- **generic** — Minimal

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
    locale.nix               # bingux.locale option
    fonts.nix                # bingux.fonts options
  profiles/                  # workstation, laptop, generic
  installer/
    live-iso.nix             # ISO configuration
    live-shell.nix           # Live environment shell
installer/                   # GTK4 Python graphical installer
pkgs/
  bingux-cli/                # bgx package manager
  bingux-installer/          # GTK4 installer package
  bingux-plymouth/           # Plymouth boot theme
  os-helper/                 # os CLI
files/                       # Branding, fonts, fastfetch config
overlays/                    # Nixpkgs overlays
```

## License

MIT
