{ writeShellScriptBin, fastfetch }:
writeShellScriptBin "fastfetch" ''
    LOGO="/etc/bingus-fastfetch.png"
    TERM_PROG="''${TERM_PROGRAM:-}"
    TERM_NAME="''${TERM:-}"

    # Terminals that support kitty image protocol
    if [ "$TERM_PROG" = "ghostty" ] || \
       [ "$TERM_PROG" = "WezTerm" ] || \
       [ "$TERM_NAME" = "xterm-kitty" ] || \
       [ -n "''${KITTY_PID:-}" ]; then
        exec ${fastfetch}/bin/fastfetch \
            --logo "$LOGO" \
            --logo-type kitty-direct \
            --logo-width 30 \
            --logo-height 15 \
            "$@"
    fi

    # Terminals that support sixel
    if [ "$TERM_PROG" = "foot" ] || \
       [ "$TERM_PROG" = "contour" ] || \
       [ "$TERM_PROG" = "mlterm" ]; then
        exec ${fastfetch}/bin/fastfetch \
            --logo "$LOGO" \
            --logo-type sixel \
            --logo-width 30 \
            --logo-height 15 \
            "$@"
    fi

    # Fallback: ASCII bingus cat
    exec ${fastfetch}/bin/fastfetch \
        --logo /etc/bingus.ascii \
        --logo-type file \
        --logo-color-1 "magenta" \
        "$@"
''
