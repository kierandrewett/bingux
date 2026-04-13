{ writeShellScriptBin }:
writeShellScriptBin "os" ''
    set -euo pipefail

    HOST="$(hostname)"
    OS_DIR="/os"
    FLAKE="$OS_DIR#$HOST"

    case "''${1:-}" in
        rebuild)
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        update)
            nix flake update "$OS_DIR"
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        edit)
            cd "$OS_DIR" && exec "''${EDITOR:-nano}" .
            ;;
        *)
            echo "Usage: os <rebuild|update|edit>"
            ;;
    esac
''
