{ writeShellScriptBin }:
writeShellScriptBin "os" ''
    set -euo pipefail

    HOST="$(hostname)"
    FLAKE="/os#$HOST"

    case "''${1:-}" in
        rebuild)
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        test)
            sudo nixos-rebuild test --flake "$FLAKE"
            ;;
        update)
            nix flake update /os
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        *)
            echo "Usage: os <rebuild|test|update>"
            ;;
    esac
''
