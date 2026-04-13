{ lib, ... }:
{
    options.monitors = lib.mkOption {
        type = lib.types.attrs;
        default = { };
        description = "Per-host monitor metadata consumed by shared modules.";
    };
}
