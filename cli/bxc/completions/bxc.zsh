#compdef bxc

# zsh completion for bxc — Bingux sandbox runtime CLI

_bxc() {
    local -a commands
    commands=(
        'run:Run a package in a sandbox'
        'shell:Open an interactive shell in a package sandbox'
        'inspect:Show sandbox configuration for a package'
        'perms:Show or manage permissions for a package'
        'ps:List running sandboxed processes'
        'ls:List per-package home contents'
        'mounts:Show the computed mount set for a package sandbox'
    )

    _arguments -C \
        '1:command:->command' \
        '*::arg:->args'

    case "$state" in
        command)
            _describe -t commands 'bxc command' commands
            ;;
        args)
            case "${words[1]}" in
                run)
                    _arguments \
                        '1:package:' \
                        '*:arguments:'
                    ;;
                shell|inspect|ls|mounts)
                    _arguments '1:package:'
                    ;;
                perms)
                    _arguments \
                        '--reset[Reset all permissions to defaults]' \
                        '1:package:'
                    ;;
            esac
            ;;
    esac
}

_bxc "$@"
