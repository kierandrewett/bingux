#compdef bsys

# zsh completion for bsys — Bingux system manager

_bsys() {
    local -a commands
    commands=(
        'add:Install a system package (volatile by default)'
        'rm:Remove a system package'
        'keep:Promote a volatile package to persistent'
        'unkeep:Demote a persistent package to volatile'
        'build:Build a package from a BPKGBUILD recipe'
        'upgrade:Upgrade system packages'
        'apply:Recompose the system profile'
        'rollback:Roll back to a previous system generation'
        'history:List system generations'
        'diff:Diff two system generations'
        'list:List installed system packages'
        'info:Show details for a system package'
        'grant:Pre-grant permissions for a system service'
        'revoke:Revoke permissions for a system service'
        'gc:Garbage collect the package store'
        'export:Export packages as .bgx archives'
        'repo:Manage package repositories'
        'home:System-level home configuration convergence'
    )

    _arguments -C \
        '1:command:->command' \
        '*::arg:->args'

    case "$state" in
        command)
            _describe -t commands 'bsys command' commands
            ;;
        args)
            case "${words[1]}" in
                add)
                    _arguments \
                        '--keep[Persist across reboots]' \
                        '1:package:'
                    ;;
                rm|keep|build|info)
                    _arguments '1:package:'
                    ;;
                unkeep)
                    _arguments \
                        '--force[Force even for boot_essential packages]' \
                        '1:package:'
                    ;;
                upgrade)
                    _arguments \
                        '--all[Upgrade all packages]' \
                        '1:package:'
                    ;;
                diff)
                    _arguments \
                        '1:generation 1:' \
                        '2:generation 2:'
                    ;;
                rollback)
                    _arguments '1:generation number:'
                    ;;
                grant|revoke)
                    _arguments \
                        '1:package:' \
                        '*:permission:'
                    ;;
                gc)
                    _arguments '--dry-run[Only show what would be removed]'
                    ;;
                export)
                    _arguments \
                        '--all[Export all packages]' \
                        '--index[Generate index.toml in directory]:directory:_directories' \
                        '1:package:'
                    ;;
                repo)
                    local -a repo_commands
                    repo_commands=(
                        'add:Add a repository'
                        'rm:Remove a repository'
                        'sync:Sync all repositories'
                    )
                    _describe -t commands 'repo subcommand' repo_commands
                    ;;
                home)
                    local -a home_commands
                    home_commands=(
                        'apply:Apply home configuration'
                    )
                    _describe -t commands 'home subcommand' home_commands
                    ;;
            esac
            ;;
    esac
}

_bsys "$@"
