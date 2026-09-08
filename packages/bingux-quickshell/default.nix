{ quickshell }:
quickshell.overrideAttrs (previous: {
    patches = (previous.patches or []) ++ [ ./layer-initial-state.patch ];
    # systemd restarts the shell; repeated crashes must not produce memory dumps.
    cmakeFlags = (previous.cmakeFlags or []) ++ [ "-DCRASH_REPORTER=OFF" ];
})
