# bash completion for bsys — Bingux system manager

_bsys() {
    local cur prev commands
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="add rm keep unkeep build upgrade apply rollback history diff list info grant revoke gc export repo home"

    case "$prev" in
        bsys)
            COMPREPLY=($(compgen -W "$commands" -- "$cur"))
            ;;
        repo)
            COMPREPLY=($(compgen -W "add rm sync" -- "$cur"))
            ;;
        home)
            COMPREPLY=($(compgen -W "apply" -- "$cur"))
            ;;
        add)
            COMPREPLY=($(compgen -W "--keep" -- "$cur"))
            ;;
        unkeep)
            COMPREPLY=($(compgen -W "--force" -- "$cur"))
            ;;
        upgrade)
            COMPREPLY=($(compgen -W "--all" -- "$cur"))
            ;;
        gc)
            COMPREPLY=($(compgen -W "--dry-run" -- "$cur"))
            ;;
        export)
            COMPREPLY=($(compgen -W "--all --index" -- "$cur"))
            ;;
        add|rm|keep|unkeep|build|info|grant|revoke)
            # Package name completion would query the store
            COMPREPLY=()
            ;;
    esac
}
complete -F _bsys bsys
