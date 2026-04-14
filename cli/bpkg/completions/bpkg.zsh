#compdef bpkg

# zsh completion for bpkg — Bingux user package manager

_bpkg() {
    local -a commands
    commands=(
        'add:Install a package (volatile by default, --keep for persistent)'
        'rm:Remove a package from the user profile'
        'keep:Promote a volatile package to persistent'
        'unkeep:Demote a persistent package to volatile'
        'pin:Pin a package to a specific version'
        'unpin:Remove a version pin from a package'
        'upgrade:Upgrade packages'
        'list:List installed user packages'
        'search:Search available packages'
        'info:Show package details'
        'grant:Pre-grant permissions to a package'
        'revoke:Revoke permissions from a package'
        'apply:Recompose the user profile from declared state'
        'rollback:Roll back the user profile to a previous generation'
        'history:List user profile generations'
        'init:First-time user profile setup'
        'home:Manage home environment (home.toml)'
        'repo:Manage package repositories'
    )

    _arguments -C \
        '1:command:->command' \
        '*::arg:->args'

    case "$state" in
        command)
            _describe -t commands 'bpkg command' commands
            ;;
        args)
            case "${words[1]}" in
                add)
                    _arguments \
                        '--keep[Make the install persistent across reboots]' \
                        '1:package:'
                    ;;
                rm)
                    _arguments \
                        '--purge[Also delete per-package state]' \
                        '1:package:'
                    ;;
                upgrade)
                    _arguments \
                        '--all[Upgrade all user packages]' \
                        '1:package:'
                    ;;
                keep|unkeep|unpin|info)
                    _arguments '1:package:'
                    ;;
                pin)
                    _arguments '1:spec (pkg=version):'
                    ;;
                search)
                    _arguments '1:query:'
                    ;;
                grant|revoke)
                    _arguments \
                        '1:package:' \
                        '*:permission:'
                    ;;
                rollback)
                    _arguments '1:generation number:'
                    ;;
                home)
                    local -a home_commands
                    home_commands=(
                        'apply:Converge full environment to home.toml'
                        'diff:Show what would change'
                        'status:Show current state vs declared'
                    )
                    _describe -t commands 'home subcommand' home_commands
                    ;;
                repo)
                    local -a repo_commands
                    repo_commands=(
                        'list:List configured repositories'
                        'add:Add a user repository'
                        'rm:Remove a user repository'
                        'sync:Refresh repository indexes'
                    )
                    _describe -t commands 'repo subcommand' repo_commands
                    ;;
            esac
            ;;
    esac
}

_bpkg "$@"
