{ writeShellScriptBin }:
writeShellScriptBin "bgx" ''
    set -euo pipefail

    case "''${1:-}" in
        install)
            shift
            if [[ "''${1:-}" == "--save" ]]; then
                shift
                pkg="''${1:?Package name required}"
                echo "Installing $pkg permanently..."
                nix profile install "nixpkgs#$pkg"
            else
                pkg="''${1:?Package name required}"
                echo "Installing $pkg for this session..."
                exec nix shell "nixpkgs#$pkg"
            fi
            ;;
        remove)
            shift
            pkg="''${1:?Package name required}"
            nix profile remove ".*$pkg.*" 2>/dev/null || true
            echo "Removed $pkg."
            ;;
        search)
            shift
            nix search nixpkgs "''${1:?Query required}"
            ;;
        list)
            nix profile list 2>/dev/null
            ;;
        *)
            echo "Usage: bgx <install [--save]|remove|search|list> <package>"
            echo ""
            echo "  install <pkg>          Use a package for this session only"
            echo "  install --save <pkg>   Install permanently (persists after reboot)"
            echo "  remove <pkg>           Remove a saved package"
            echo "  search <query>         Search for packages"
            echo "  list                   List saved packages"
            ;;
    esac
''
