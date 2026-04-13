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
                f="/os/extra-packages.nix"
                if [[ ! -f "$f" ]]; then
                    printf '{ pkgs, ... }:\n{ environment.systemPackages = with pkgs; [\n]; }\n' > "$f"
                fi
                grep -qx "    $pkg" "$f" 2>/dev/null || sed -i "/^\]; }$/i\\    $pkg" "$f"
                echo "Saved. Run 'os rebuild' to apply."
            else
                pkg="''${1:?Package name required}"
                echo "Installing $pkg for this session..."
                exec nix shell "nixpkgs#$pkg"
            fi
            ;;
        remove)
            shift
            pkg="''${1:?Package name required}"
            [[ -f /os/extra-packages.nix ]] && sed -i "/^    $pkg$/d" /os/extra-packages.nix 2>/dev/null || true
            echo "Removed $pkg. Run 'os rebuild' to apply."
            ;;
        search)
            shift
            nix search nixpkgs "''${1:?Query required}"
            ;;
        *)
            echo "Usage: bgx <install [--save]|remove|search> <package>"
            echo ""
            echo "  install <pkg>          Use a package for this session only"
            echo "  install --save <pkg>   Add to system config permanently"
            echo "  remove <pkg>           Remove from system config"
            echo "  search <query>         Search for packages"
            ;;
    esac
''
