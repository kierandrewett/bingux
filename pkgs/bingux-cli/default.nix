{ writeShellScriptBin }:
writeShellScriptBin "bgx" ''
    set -euo pipefail

    # Volatile profile — cleared on every boot via tmpfiles
    VOLATILE_PROFILE="/nix/var/nix/profiles/per-user/$USER/bgx-volatile"

    case "''${1:-}" in
        install)
            shift
            if [[ "''${1:-}" == "--save" ]]; then
                shift
                pkg="''${1:?Package name required}"
                echo "Installing $pkg permanently..."
                nix profile install --profile "$HOME/.local/state/nix/profiles/profile" "nixpkgs#$pkg"
            else
                pkg="''${1:?Package name required}"
                echo "Installing $pkg (until reboot)..."
                nix profile install --profile "$VOLATILE_PROFILE" "nixpkgs#$pkg"
                echo "Installed. Available system-wide until reboot."
            fi
            ;;
        remove)
            shift
            pkg="''${1:?Package name required}"
            # Try both profiles
            nix profile remove --profile "$VOLATILE_PROFILE" ".*$pkg.*" 2>/dev/null || true
            nix profile remove --profile "$HOME/.local/state/nix/profiles/profile" ".*$pkg.*" 2>/dev/null || true
            echo "Removed $pkg."
            ;;
        search)
            shift
            nix search nixpkgs "''${1:?Query required}"
            ;;
        list)
            echo "=== Temporary (until reboot) ==="
            nix profile list --profile "$VOLATILE_PROFILE" 2>/dev/null || echo "(none)"
            echo ""
            echo "=== Permanent ==="
            nix profile list 2>/dev/null || echo "(none)"
            ;;
        *)
            echo "Usage: bgx <install [--save]|remove|search|list> <package>"
            echo ""
            echo "  install <pkg>          Install until reboot (available everywhere)"
            echo "  install --save <pkg>   Install permanently (persists after reboot)"
            echo "  remove <pkg>           Remove a package"
            echo "  search <query>         Search for packages"
            echo "  list                   List installed packages"
            ;;
    esac
''
