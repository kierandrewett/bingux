{ pkgs }:
{
    shellAliases = {
        gs = "git status";
        gd = "git diff";
        gp = "git push";
        gl = "git pull --ff-only";

        cat = "bat";
        grep = "rg";
        find = "fd";
        ff = "fastfetch";
        ".." = "cd ..";
        "..." = "cd ../..";
    };

    zshInit = ''
        eval "$(${pkgs.zoxide}/bin/zoxide init zsh)"

        # Key bindings for Ctrl+Arrow, Home, End, Delete
        bindkey '^[[1;5C' forward-word        # Ctrl+Right
        bindkey '^[[1;5D' backward-word       # Ctrl+Left
        bindkey '^[[H'    beginning-of-line   # Home
        bindkey '^[[F'    end-of-line         # End
        bindkey '^[[3~'   delete-char         # Delete
    '';

    cliPackages = with pkgs; [
        comma
        ripgrep
        fd
        fzf
        bat
        eza
        zoxide
        fastfetch
    ];

    fastfetchConfigSource = ../../files/fastfetch/config.jsonc;

    gitExtraConfig = {
        init.defaultBranch = "main";
        core.tabwidth = 4;
        pull.ff = "only";
    };

    gitDeltaOptions = {
        side-by-side = true;
        navigate = true;
        line-numbers = true;
    };
}
