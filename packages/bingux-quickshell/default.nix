{ quickshell }:
quickshell.overrideAttrs (previous: {
    patches = (previous.patches or []) ++ [ ./layer-initial-state.patch ];
})
