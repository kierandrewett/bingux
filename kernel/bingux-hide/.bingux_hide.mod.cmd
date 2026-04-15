savedcmd_bingux_hide.mod := printf '%s\n'   bingux_hide.o | awk '!x[$$0]++ { print("./"$$0) }' > bingux_hide.mod
