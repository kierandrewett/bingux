#!/usr/bin/env bash
# Run in the Bingux session. --activation also checks startup from a stopped shell.
set -euo pipefail
if [[ "${1:-}" == "--activation" ]]; then
    systemctl --user stop quickshell.service
    trap 'systemctl --user start quickshell.service' EXIT
elif [[ $# -ne 0 ]]; then
    echo "Usage: $0 [--activation]" >&2
    exit 2
fi
server="$(busctl --user --timeout=30 call org.freedesktop.Notifications \
    /org/freedesktop/Notifications org.freedesktop.Notifications GetServerInformation)"
[[ "$server" == 'ssss "quickshell" '* ]]
owner="$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus \
    org.freedesktop.DBus GetConnectionUnixProcessID s org.freedesktop.Notifications)"
pid="$(systemctl --user show quickshell.service --property=MainPID --value)"
[[ "$pid" != 0 && "$owner" == "u $pid" ]]
[[ "$(systemctl --user show quickshell.service --property=Type --value)" == dbus ]]
systemctl --user is-active --quiet quickshell.service
echo "PASS: Bingux owns notifications through quickshell.service (PID $pid)"
