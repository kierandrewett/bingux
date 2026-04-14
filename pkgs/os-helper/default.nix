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
        upgrade)
            sudo nix flake update /os
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        *)
            echo "Usage: os <rebuild|test|upgrade>"
            echo ""
            echo "  rebuild    Rebuild from local config"
            echo "  test       Rebuild and test (no bootloader update)"
            echo "  upgrade    Update flake inputs (bingux, nixpkgs, etc.) and rebuild"
            ;;
    esac
''
