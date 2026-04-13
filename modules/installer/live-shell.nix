{ lib, pkgs, ... }:
let
    zshProfile = import ../shared/zsh-profile.nix {
        inherit pkgs;
    };
in
{
    users.users.nixos = {
        initialHashedPassword = lib.mkForce null;
        initialPassword = "nixos";
        shell = pkgs.zsh;
        extraGroups = [ "networkmanager" "wheel" ];
    };

    programs.zsh = {
        enable = true;
        enableCompletion = true;
        shellAliases = zshProfile.shellAliases;
        interactiveShellInit = zshProfile.zshInit;
    };

    programs.bat.enable = true;
    programs.fzf = {
        keybindings = true;
        fuzzyCompletion = true;
    };
    programs.zoxide.enable = true;

    programs.direnv = {
        enable = true;
        nix-direnv.enable = true;
    };

    programs.nix-index = {
        enable = true;
        enableZshIntegration = true;
    };

    programs.command-not-found.enable = false;

    programs.git = {
        enable = true;
        config = {
            delta = zshProfile.gitDeltaOptions;
        } // zshProfile.gitExtraConfig;
    };

    environment.etc = {
        "xdg/fastfetch/config.jsonc".source = zshProfile.fastfetchConfigSource;
"bingus.ascii".source = ../../files/branding/bingus.ascii;
    };

    environment.variables.XDG_CONFIG_DIRS = lib.mkDefault "/etc/xdg";

    environment.shellAliases.ff = "fastfetch --config /etc/xdg/fastfetch/config.jsonc";

    environment.systemPackages = with pkgs; [
        git
        gh
        curl
        jq
        delta
    ] ++ zshProfile.cliPackages;
}
