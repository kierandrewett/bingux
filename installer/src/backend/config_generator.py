import os


def generate_config(state, dest="/tmp/bingux-os"):
    """Generate a minimal NixOS flake config for a fresh install using bingux.* options."""
    os.makedirs(os.path.join(dest, f"machines/{state.hostname}"), exist_ok=True)

    # Build bingux.* option lines
    bingux_opts = []
    if state.desktop:
        bingux_opts.append(f'        bingux.desktop = "{state.desktop}";')
    if state.locale:
        bingux_opts.append(f'        bingux.locale = "{state.locale}";')

    bingux_block = "\n".join(bingux_opts)

    user_block = ""
    if state.username:
        user_block = f"""
        users.users.{state.username} = {{
            isNormalUser = true;
            description = "{state.username}";
            shell = pkgs.zsh;
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

    machine_nix = f"""{{ pkgs, ... }}:
{{
    imports = [ ./hardware-configuration.nix ];

    networking.hostName = "{state.hostname}";

{bingux_block}
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
