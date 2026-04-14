# fish completion for bpkg — Bingux user package manager

# Disable file completions by default
complete -c bpkg -f

# Top-level subcommands
complete -c bpkg -n __fish_use_subcommand -a add       -d 'Install a package (volatile by default)'
complete -c bpkg -n __fish_use_subcommand -a rm        -d 'Remove a package from the user profile'
complete -c bpkg -n __fish_use_subcommand -a keep      -d 'Promote a volatile package to persistent'
complete -c bpkg -n __fish_use_subcommand -a unkeep    -d 'Demote a persistent package to volatile'
complete -c bpkg -n __fish_use_subcommand -a pin       -d 'Pin a package to a specific version'
complete -c bpkg -n __fish_use_subcommand -a unpin     -d 'Remove a version pin from a package'
complete -c bpkg -n __fish_use_subcommand -a upgrade   -d 'Upgrade packages'
complete -c bpkg -n __fish_use_subcommand -a list      -d 'List installed user packages'
complete -c bpkg -n __fish_use_subcommand -a search    -d 'Search available packages'
complete -c bpkg -n __fish_use_subcommand -a info      -d 'Show package details'
complete -c bpkg -n __fish_use_subcommand -a grant     -d 'Pre-grant permissions to a package'
complete -c bpkg -n __fish_use_subcommand -a revoke    -d 'Revoke permissions from a package'
complete -c bpkg -n __fish_use_subcommand -a apply     -d 'Recompose the user profile'
complete -c bpkg -n __fish_use_subcommand -a rollback  -d 'Roll back to a previous generation'
complete -c bpkg -n __fish_use_subcommand -a history   -d 'List user profile generations'
complete -c bpkg -n __fish_use_subcommand -a init      -d 'First-time user profile setup'
complete -c bpkg -n __fish_use_subcommand -a home      -d 'Manage home environment'
complete -c bpkg -n __fish_use_subcommand -a repo      -d 'Manage package repositories'

# add flags
complete -c bpkg -n '__fish_seen_subcommand_from add' -l keep -d 'Make the install persistent across reboots'

# rm flags
complete -c bpkg -n '__fish_seen_subcommand_from rm' -l purge -d 'Also delete per-package state'

# upgrade flags
complete -c bpkg -n '__fish_seen_subcommand_from upgrade' -l all -d 'Upgrade all user packages'

# home subcommands
complete -c bpkg -n '__fish_seen_subcommand_from home' -a apply  -d 'Converge full environment to home.toml'
complete -c bpkg -n '__fish_seen_subcommand_from home' -a diff   -d 'Show what would change'
complete -c bpkg -n '__fish_seen_subcommand_from home' -a status -d 'Show current state vs declared'

# repo subcommands
complete -c bpkg -n '__fish_seen_subcommand_from repo' -a list -d 'List configured repositories'
complete -c bpkg -n '__fish_seen_subcommand_from repo' -a add  -d 'Add a user repository'
complete -c bpkg -n '__fish_seen_subcommand_from repo' -a rm   -d 'Remove a user repository'
complete -c bpkg -n '__fish_seen_subcommand_from repo' -a sync -d 'Refresh repository indexes'
