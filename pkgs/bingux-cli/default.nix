{ writeShellScriptBin }:
writeShellScriptBin "bgx" ''
    set -euo pipefail

    case "''${1:-}" in
        install)
            shift
            if [[ "''${1:-}" == "--save" ]]; then
                shift
                pkg="''${1:?Package name required}"
                echo "Adding $pkg to system config..."
                nix profile install "nixpkgs#$pkg"
                # Append to /os extra-packages.nix
                f="/os/extra-packages.nix"
                if [[ ! -f "$f" ]]; then
                    printf '{ pkgs, ... }:\n{ environment.systemPackages = with pkgs; [\n]; }\n' > "$f"
                fi
                grep -qx "    $pkg" "$f" 2>/dev/null || sed -i "/^\]; }$/i\\    $pkg" "$f"
                echo "Installed now and saved to config. Run 'os rebuild' when ready."
            else
                pkg="''${1:?Package name required}"
                nix profile install "nixpkgs#$pkg"
            fi
            ;;
        remove)
            shift
            pkg="''${1:?Package name required}"
            nix profile remove ".*$pkg.*" 2>/dev/null || true
            [[ -f /os/extra-packages.nix ]] && sed -i "/^    $pkg$/d" /os/extra-packages.nix 2>/dev/null || true
            ;;
        try)
            shift
            exec nix shell "nixpkgs#''${1:?Package name required}"
            ;;
        search)
            shift
            nix search nixpkgs "''${1:?Query required}"
            ;;
        list)
            nix profile list 2>/dev/null
            ;;
        *)
            echo "Usage: bgx <install [--save]|remove|try|search|list> <package>"
            ;;
    esac
''
