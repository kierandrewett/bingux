# bash completion for bpkg — Bingux user package manager

_bpkg() {
    local cur prev commands
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="add rm keep unkeep pin unpin upgrade list search info grant revoke apply rollback history init home repo"

    case "$prev" in
        bpkg)
            COMPREPLY=($(compgen -W "$commands" -- "$cur"))
            ;;
        home)
            COMPREPLY=($(compgen -W "apply diff status" -- "$cur"))
            ;;
        repo)
            COMPREPLY=($(compgen -W "list add rm sync" -- "$cur"))
            ;;
        add)
            COMPREPLY=($(compgen -W "--keep" -- "$cur"))
            ;;
        rm)
            COMPREPLY=($(compgen -W "--purge" -- "$cur"))
            ;;
        upgrade)
            COMPREPLY=($(compgen -W "--all" -- "$cur"))
            ;;
        add|rm|keep|unkeep|pin|unpin|upgrade|info|grant|revoke)
            # Package name completion would query the store
            COMPREPLY=()
            ;;
    esac
}
complete -F _bpkg bpkg
