{ writeShellScriptBin }:
writeShellScriptBin "os" ''
    set -euo pipefail

    # Ensure /os is safe for git
    git config --global --replace-all safe.directory /os 2>/dev/null || git config --global --add safe.directory /os 2>/dev/null || true

    HOST="$(hostname)"
    OS_DIR="/os"
    FLAKE="$OS_DIR#$HOST"

    ensure_gh_auth() {
        if ! gh auth status &>/dev/null; then
            echo "GitHub not authenticated. Run: gh auth login"
            return 0
        fi
        local token
        token=$(gh auth token 2>/dev/null || true)
        if [[ -n "$token" ]] && ! grep -q "github.com" /etc/nix/nix.conf 2>/dev/null; then
            echo "access-tokens = github.com=$token" | sudo tee -a /etc/nix/nix.conf >/dev/null 2>&1 || true
        fi
    }

    usage() {
        echo "Usage: os <command>"
        echo ""
        echo "Commands:"
        echo "  rebuild, switch    Rebuild and switch to new config (restarts services)"
        echo "  apply              Apply config without restarting services"
        echo "  test               Rebuild and test (no bootloader update)"
        echo "  update             Update flake inputs and rebuild"
        echo "  edit               Open the OS config in your editor"
        echo "  diff               Show uncommitted changes"
        echo "  log                Show recent commits"
        echo "  commit [msg]       Stage all changes and commit"
        echo "  push               Commit and push to remote"
        echo "  cd                 Print the OS directory (use: cd \$(os cd))"
        echo "  status             Show git status"
        echo ""
    }

    case "''${1:-}" in
        rebuild|switch)
            ensure_gh_auth
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        apply)
            ensure_gh_auth
            sudo nixos-rebuild test --flake "$FLAKE"
            echo "Applied without restart. Run 'os rebuild' to make permanent."
            ;;
        test)
            ensure_gh_auth
            sudo nixos-rebuild test --flake "$FLAKE"
            ;;
        update)
            ensure_gh_auth
            nix flake update "$OS_DIR"
            sudo nixos-rebuild switch --flake "$FLAKE"
            ;;
        edit)
            code "$OS_DIR"
            ;;
        diff)
            git -C "$OS_DIR" diff
            ;;
        log)
            git -C "$OS_DIR" log --oneline -20
            ;;
        commit)
            cd "$OS_DIR"
            git add -A
            if [[ -n "''${2:-}" ]]; then
                shift
                git commit -m "$*"
            else
                git commit
            fi
            ;;
        push)
            cd "$OS_DIR"
            git add -A
            git commit && git push
            ;;
        cd)
            echo "$OS_DIR"
            ;;
        status|st)
            git -C "$OS_DIR" status
            ;;
        *)
            usage
            ;;
    esac
''
