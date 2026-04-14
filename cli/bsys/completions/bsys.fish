# fish completion for bsys — Bingux system manager

# Disable file completions by default
complete -c bsys -f

# Top-level subcommands
complete -c bsys -n __fish_use_subcommand -a add       -d 'Install a system package (volatile by default)'
complete -c bsys -n __fish_use_subcommand -a rm        -d 'Remove a system package'
complete -c bsys -n __fish_use_subcommand -a keep      -d 'Promote a volatile package to persistent'
complete -c bsys -n __fish_use_subcommand -a unkeep    -d 'Demote a persistent package to volatile'
complete -c bsys -n __fish_use_subcommand -a build     -d 'Build a package from a BPKGBUILD recipe'
complete -c bsys -n __fish_use_subcommand -a upgrade   -d 'Upgrade system packages'
complete -c bsys -n __fish_use_subcommand -a apply     -d 'Recompose the system profile'
complete -c bsys -n __fish_use_subcommand -a rollback  -d 'Roll back to a previous system generation'
complete -c bsys -n __fish_use_subcommand -a history   -d 'List system generations'
complete -c bsys -n __fish_use_subcommand -a diff      -d 'Diff two system generations'
complete -c bsys -n __fish_use_subcommand -a list      -d 'List installed system packages'
complete -c bsys -n __fish_use_subcommand -a info      -d 'Show details for a system package'
complete -c bsys -n __fish_use_subcommand -a grant     -d 'Pre-grant permissions for a system service'
complete -c bsys -n __fish_use_subcommand -a revoke    -d 'Revoke permissions for a system service'
complete -c bsys -n __fish_use_subcommand -a gc        -d 'Garbage collect the package store'
complete -c bsys -n __fish_use_subcommand -a export    -d 'Export packages as .bgx archives'
complete -c bsys -n __fish_use_subcommand -a repo      -d 'Manage package repositories'
complete -c bsys -n __fish_use_subcommand -a home      -d 'System-level home configuration'

# add flags
complete -c bsys -n '__fish_seen_subcommand_from add' -l keep -d 'Persist across reboots'

# unkeep flags
complete -c bsys -n '__fish_seen_subcommand_from unkeep' -l force -d 'Force even for boot_essential packages'

# upgrade flags
complete -c bsys -n '__fish_seen_subcommand_from upgrade' -l all -d 'Upgrade all packages'

# gc flags
complete -c bsys -n '__fish_seen_subcommand_from gc' -l dry-run -d 'Only show what would be removed'

# export flags
complete -c bsys -n '__fish_seen_subcommand_from export' -l all -d 'Export all packages'
complete -c bsys -n '__fish_seen_subcommand_from export' -l index -d 'Generate index.toml in directory'

# repo subcommands
complete -c bsys -n '__fish_seen_subcommand_from repo' -a add  -d 'Add a repository'
complete -c bsys -n '__fish_seen_subcommand_from repo' -a rm   -d 'Remove a repository'
complete -c bsys -n '__fish_seen_subcommand_from repo' -a sync -d 'Sync all repositories'

# home subcommands
complete -c bsys -n '__fish_seen_subcommand_from home' -a apply -d 'Apply home configuration'
