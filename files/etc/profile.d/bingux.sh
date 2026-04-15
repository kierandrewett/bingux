# Bingux environment setup
export PATH="/system/profiles/current/bin:/bin:/sbin:/usr/bin:/usr/sbin"
export LD_LIBRARY_PATH="/lib64:/usr/lib64"
export BPKG_STORE_ROOT="/system/packages"
export BSYS_CONFIG_PATH="/system/config/system.toml"
export BSYS_PROFILES_ROOT="/system/profiles"
export BSYS_PACKAGES_ROOT="/system/packages"
export SSL_CERT_FILE="/etc/ssl/certs/ca-bundle.crt"
export BINGUX_VERSION="0.1.0"

# Prompt
if [ "$(id -u)" -eq 0 ]; then
    PS1='\[\e[1;31m\]bingux\[\e[0m\]:\[\e[1;34m\]\w\[\e[0m\]# '
else
    PS1='\[\e[1;36m\]bingux\[\e[0m\]:\[\e[1;34m\]\w\[\e[0m\]$ '
fi

# Welcome on login
_gen=$(readlink /system/profiles/current 2>/dev/null || echo "?")
_pkgs=$(ls /system/packages/ 2>/dev/null | wc -l)
echo "  Bingux ${BINGUX_VERSION} | ${_pkgs} packages | generation ${_gen}"
unset _gen _pkgs
