import os


def generate_config(state, dest="/tmp/bingux-os"):
    """Generate a minimal NixOS flake config for a fresh install."""
    os.makedirs(os.path.join(dest, f"machines/{state.hostname}"), exist_ok=True)

    desktop_line = ""
    if state.desktop:
        desktop_line = f'        bingux.desktop = "{state.desktop}";'

    locale_line = ""
    if state.locale:
        locale_line = f'        i18n.defaultLocale = "{state.locale}";'

    keymap_line = ""
    if state.keymap:
        keymap_line = f'        console.keyMap = "{state.keymap}";'

    user_block = ""
    if state.username:
        user_block = f"""
        users.users.{state.username} = {{
            isNormalUser = true;
            description = "{state.username}";
            extraGroups = [ "wheel" "networkmanager" "audio" "video" ];
        }};
        users.mutableUsers = true;"""

    flake_nix = f"""{{
    description = "{state.hostname} - Bingux NixOS configuration";

    inputs = {{
        bingux.url = "github:kierandrewett/bingux";
        nixpkgs.follows = "bingux/nixpkgs";
    }};

    outputs = inputs@{{ bingux, ... }}: {{
        nixosConfigurations.{state.hostname} = bingux.lib.mkBinguxHost {{
            hostname = "{state.hostname}";
            profile = "{state.profile}";
            extraModules = [
                ./machines/{state.hostname}
            ];
        }};
    }};
}}
"""

    machine_nix = f"""{{ ... }}:
{{
    imports = [ ./hardware-configuration.nix ];
{desktop_line}
{locale_line}
{keymap_line}
{user_block}
    system.stateVersion = "25.05";
}}
"""

    with open(os.path.join(dest, "flake.nix"), "w") as f:
        f.write(flake_nix)

    with open(os.path.join(dest, f"machines/{state.hostname}/default.nix"), "w") as f:
        f.write(machine_nix)

    # Stub hardware-configuration.nix (replaced by nixos-generate-config)
    hw_stub = os.path.join(dest, f"machines/{state.hostname}/hardware-configuration.nix")
    with open(hw_stub, "w") as f:
        f.write("{ ... }:\n{ imports = []; }\n")

    # Init git repo (nix flake requires it)
    os.system(f"cd {dest} && git init -b main && git add -A && "
              f'git -c user.name=bingux -c user.email=bingux@localhost '
              f'commit -m "Initial Bingux configuration" --allow-empty')
