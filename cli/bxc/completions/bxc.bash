# bash completion for bxc — Bingux sandbox runtime CLI

_bxc() {
    local cur prev commands
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="run shell inspect perms ps ls mounts"

    case "$prev" in
        bxc)
            COMPREPLY=($(compgen -W "$commands" -- "$cur"))
            ;;
        perms)
            COMPREPLY=($(compgen -W "--reset" -- "$cur"))
            ;;
        run|shell|inspect|perms|ls|mounts)
            # Package name completion would query the store
            COMPREPLY=()
            ;;
    esac
}
complete -F _bxc bxc
