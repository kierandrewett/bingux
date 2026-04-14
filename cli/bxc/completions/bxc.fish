# fish completion for bxc — Bingux sandbox runtime CLI

# Disable file completions by default
complete -c bxc -f

# Top-level subcommands
complete -c bxc -n __fish_use_subcommand -a run     -d 'Run a package in a sandbox'
complete -c bxc -n __fish_use_subcommand -a shell   -d 'Open an interactive shell in a package sandbox'
complete -c bxc -n __fish_use_subcommand -a inspect -d 'Show sandbox configuration for a package'
complete -c bxc -n __fish_use_subcommand -a perms   -d 'Show or manage permissions for a package'
complete -c bxc -n __fish_use_subcommand -a ps      -d 'List running sandboxed processes'
complete -c bxc -n __fish_use_subcommand -a ls      -d 'List per-package home contents'
complete -c bxc -n __fish_use_subcommand -a mounts  -d 'Show the computed mount set for a package sandbox'

# perms flags
complete -c bxc -n '__fish_seen_subcommand_from perms' -l reset -d 'Reset all permissions to defaults'
