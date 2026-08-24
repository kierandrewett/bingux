{
    config,
    lib,
    pkgs,
    ...
}:
let
    cfg = config.bingux.secrets;

    ageBootstrap = pkgs.writeShellApplication {
        name = "bingux-secrets-init";
        runtimeInputs = [
            pkgs.age
            pkgs.coreutils
        ];
        text = ''
            key_file=${lib.escapeShellArg cfg.age.keyFile}

            if test -e "$key_file"; then
                printf '[secrets] Refusing to replace existing age key: %s\n' "$key_file" >&2
                exit 1
            fi

            install -d -m 0700 "$(dirname "$key_file")"
            age-keygen -o "$key_file" >/dev/null
            chmod 0600 "$key_file"

            printf '[secrets] Add this public recipient to the profile .sops.yaml file:\n'
            age-keygen -y "$key_file"
        '';
    };
in
{
    options.bingux.secrets = {
        enable = lib.mkEnableOption "profile-managed SOPS secrets";

        defaultSopsFile = lib.mkOption {
            type = lib.types.nullOr lib.types.path;
            default = null;
            description = "The encrypted SOPS file owned by the selected profile.";
        };

        age = {
            keyFile = lib.mkOption {
                type = lib.types.strMatching "^/.*";
                default = "/var/lib/sops-nix/key.txt";
                description = "The absolute root-owned age private key path used to decrypt profile secrets.";
            };

            generateKey = lib.mkOption {
                type = lib.types.bool;
                default = false;
                description = "Generate an age key during secret activation when keyFile is absent. Prefer the bingux-secrets-init command for the first-host bootstrap.";
            };
        };

        entries = lib.mkOption {
            type = lib.types.attrsOf (
                lib.types.submodule (
                    { name, ... }:
                    {
                        options = {
                            key = lib.mkOption {
                                type = lib.types.str;
                                default = name;
                                description = "The key name in the encrypted SOPS file.";
                            };

                            owner = lib.mkOption {
                                type = lib.types.str;
                                default = "root";
                                description = "The local user that owns the decrypted secret file.";
                            };

                            group = lib.mkOption {
                                type = lib.types.str;
                                default = "root";
                                description = "The local group that owns the decrypted secret file.";
                            };

                            mode = lib.mkOption {
                                type = lib.types.str;
                                default = "0400";
                                description = "The file mode for the decrypted secret file.";
                            };

                            neededForUsers = lib.mkOption {
                                type = lib.types.bool;
                                default = false;
                                description = "Make this secret available before normal user account creation.";
                            };

                            restartUnits = lib.mkOption {
                                type = lib.types.listOf lib.types.str;
                                default = [ ];
                                description = "Systemd units to restart after the secret changes.";
                            };
                        };
                    }
                )
            );
            default = { };
            description = "The decrypted secrets declared by the selected profile.";
        };
    };

    config = lib.mkIf cfg.enable (
        lib.mkMerge [
            {
                assertions = [
                    {
                        assertion = cfg.defaultSopsFile != null || cfg.entries == { };
                        message = "bingux.secrets.defaultSopsFile must point to an encrypted profile file when secret entries are declared.";
                    }
                ]
                ++ lib.mapAttrsToList (entryName: entry: {
                    assertion = !entry.neededForUsers || (entry.owner == "root" && entry.group == "root");
                    message = "bingux.secrets.entries.${entryName} uses neededForUsers and must keep root owner and group.";
                }) cfg.entries;

                sops = {
                    age = {
                        keyFile = cfg.age.keyFile;
                        generateKey = cfg.age.generateKey;
                    };

                    secrets = lib.mapAttrs (_: secret: {
                        key = secret.key;
                        owner = secret.owner;
                        group = secret.group;
                        mode = secret.mode;
                        neededForUsers = secret.neededForUsers;
                        restartUnits = secret.restartUnits;
                    }) cfg.entries;
                };
                environment.systemPackages = [ ageBootstrap ];
            }

            (lib.mkIf (cfg.defaultSopsFile != null) {
                sops.defaultSopsFile = cfg.defaultSopsFile;
            })
        ]
    );
}
