{
    config,
    hostSystem,
    inputs,
    lib,
    pkgs,
    ...
}:
let
    cfg = config.bingux.performance;
    isX86_64 = hostSystem == "x86_64-linux";

    cachyosPackageSets = lib.optionalAttrs isX86_64 {
        cachyos-latest = "linuxPackages-cachyos-latest";
        cachyos-latest-lto = "linuxPackages-cachyos-latest-lto";
        cachyos-bore = "linuxPackages-cachyos-bore";
        cachyos-bore-lto = "linuxPackages-cachyos-bore-lto";
        cachyos-bore-lto-x86_64-v3 = "linuxPackages-cachyos-bore-lto-x86_64-v3";
    };

    nixpkgsPackageSets = lib.optionalAttrs isX86_64 {
        zen = pkgs.linuxPackages_zen;
        xanmod = pkgs.linuxPackages_xanmod;
        xanmod-latest = pkgs.linuxPackages_xanmod_latest;
        xanmod-stable = pkgs.linuxPackages_xanmod_stable;
    };

    kernelChoices =
        [ "nixpkgs" ]
        ++ builtins.attrNames nixpkgsPackageSets
        ++ builtins.attrNames cachyosPackageSets;
    isCachyosKernel = builtins.hasAttr cfg.kernel cachyosPackageSets;
    usesX86_64V3 = cfg.kernel == "cachyos-bore-lto-x86_64-v3";
    selectedKernelPackages =
        if isCachyosKernel then
            lib.attrByPath [
                "cachyosKernels"
                cachyosPackageSets.${cfg.kernel}
            ] (throw "The selected CachyOS kernel package set is unavailable") pkgs
        else
            lib.attrByPath [
                cfg.kernel
            ] (throw "The selected Nixpkgs kernel package set is unavailable") nixpkgsPackageSets;
in
{
    options.bingux.performance = {
        enable = lib.mkEnableOption "the Bingux workstation performance profile";

        kernel = lib.mkOption {
            type = lib.types.enum kernelChoices;
            default = "nixpkgs";
            description = "The kernel package set. Zen and XanMod come from Nixpkgs; CachyOS selections use its pinned upstream overlay.";
        };

        allowX86_64V3 = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Allow selecting a CachyOS kernel that requires x86-64-v3 CPU features.";
        };

        cpuGovernor = lib.mkOption {
            type = lib.types.enum [
                "performance"
                "schedutil"
            ];
            default = "schedutil";
            description = "The CPU frequency governor to use when the profile enables performance settings.";
        };

        enableAmdPstate = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Enable AMD active P-state control on compatible systems.";
        };

        zramPercent = lib.mkOption {
            type = lib.types.ints.between 1 100;
            default = 25;
            description = "The percentage of physical memory assigned to compressed ZRAM swap.";
        };

        disableCpuMitigations = lib.mkOption {
            type = lib.types.bool;
            default = false;
            description = "Disable CPU security mitigations. This stays off unless a profile explicitly accepts the security cost.";
        };
    };

    config = lib.mkMerge [
        {
            assertions = [
                {
                    assertion = !usesX86_64V3 || cfg.allowX86_64V3;
                    message = "The x86-64-v3 CachyOS kernel requires bingux.performance.allowX86_64V3 = true.";
                }
                {
                    assertion = !cfg.enableAmdPstate || hostSystem == "x86_64-linux";
                    message = "bingux.performance.enableAmdPstate is only supported on x86_64-linux hosts.";
                }
            ];
        }
        (lib.mkIf isCachyosKernel {
            nixpkgs.overlays = [ inputs.nix-cachyos-kernel.overlays.pinned ];
            nix.settings = {
                extra-substituters = [ "https://attic.xuyh0120.win/lantian" ];
                extra-trusted-public-keys = [ "lantian:EeAUQ+W+6r7EtwnmYjeVwx5kOGEBpjlBfPlzGlTNvHc=" ];
            };
        })

        (lib.mkIf (cfg.kernel != "nixpkgs") {
            boot.kernelPackages = selectedKernelPackages;
        })

        (lib.mkIf cfg.enable {
            powerManagement.cpuFreqGovernor = cfg.cpuGovernor;
            services.irqbalance.enable = true;

            zramSwap = {
                enable = true;
                algorithm = "zstd";
                memoryPercent = cfg.zramPercent;
                priority = 100;
            };

            boot.kernel.sysctl = {
                "fs.inotify.max_user_instances" = 8192;
                "fs.inotify.max_user_watches" = 1048576;
                "vm.swappiness" = 10;
                "vm.vfs_cache_pressure" = 50;
            };
        })

        (lib.mkIf (cfg.enable && isX86_64) {
            hardware.cpu.amd.updateMicrocode = lib.mkDefault true;
        })

        (lib.mkIf (cfg.enableAmdPstate && isX86_64) {
            boot.kernelParams = [ "amd_pstate=active" ];
        })


        (lib.mkIf cfg.disableCpuMitigations {
            boot.kernelParams = [ "mitigations=off" ];
            warnings = [
                "bingux.performance.disableCpuMitigations disables CPU vulnerability mitigations. Enable it only after an explicit threat-model decision."
            ];
        })
    ];
}
