#!/bin/sh
# Bingux v2 — Production Init
# Root layout: /io /system /users
# FHS compat via hidden symlinks + kernel module
set -e

# PATH must be set FIRST — before any commands
export PATH="/system/profiles/current/bin:/bin:/sbin:/usr/bin"

# ── Phase 1: Mount kernel pseudo-filesystems ──────────────────
mount -t proc proc /system/kernel/proc
mount -t sysfs sysfs /system/kernel/sys
mount -t devtmpfs devtmpfs /io 2>/dev/null || true

# Ephemeral state (cleared every boot)
mount -t tmpfs -o size=256M tmpfs /system/state/ephemeral
mkdir -p /system/state/ephemeral/bingux
mkdir -p /system/state/ephemeral/dbus

# Temp
mount -t tmpfs -o size=1500M tmpfs /system/tmp

# devpts / shm
mkdir -p /io/pts /io/shm
mount -t devpts devpts /io/pts 2>/dev/null || true
mount -t tmpfs tmpfs /io/shm 2>/dev/null || true

# ── Phase 2: Environment ──────────────────────────────────────
export LD_LIBRARY_PATH="/lib64"
export HOME="/users/root"
export TERM=linux
export BPKG_STORE_ROOT="/system/packages"
export BSYS_WORK_DIR="/system/tmp/bsys-work"
export BSYS_CACHE_DIR="/system/tmp/bsys-cache"

# ── Phase 3: Set hostname from config ─────────────────────────
if [ -f /system/config/system.toml ]; then
    HOSTNAME=$(grep '^hostname' /system/config/system.toml 2>/dev/null | head -1 | sed 's/.*= *"\(.*\)"/\1/')
    [ -n "$HOSTNAME" ] && echo "$HOSTNAME" > /system/kernel/proc/sys/kernel/hostname
fi

# ── Phase 4: Generate /etc from system config ─────────────────
# /etc is a tmpfs generated fresh each boot from /system/config/
mount -t tmpfs -o size=16M tmpfs /etc
if which bsys >/dev/null 2>&1 && [ -f /system/config/system.toml ]; then
    bsys apply 2>/dev/null || true
fi
# Ensure essential /etc files exist (fallback if bsys apply didn't run)
[ -f /etc/passwd ] || printf 'root:x:0:0:root:/users/root:/bin/sh\nnobody:x:65534:65534:Nobody:/:/sbin/nologin\n' > /etc/passwd
[ -f /etc/group ] || printf 'root:x:0:\nnobody:x:65534:\n' > /etc/group
[ -f /etc/hostname ] || echo "bingux" > /etc/hostname
[ -f /etc/os-release ] || printf 'NAME="Bingux"\nID=bingux\nVERSION_ID=2\nPRETTY_NAME="Bingux v2"\n' > /etc/os-release

# ── Phase 5: Load kernel module (hide FHS dirs) ──────────────
if [ -f /system/modules/bingux_hide.ko ]; then
    insmod /system/modules/bingux_hide.ko 2>/dev/null && true
fi

# ── Phase 6: Persistent disk ─────────────────────────────────
DISK=""
for dev in /io/vda /io/sda; do
    [ -b "$dev" ] && DISK="$dev" && break
done
if [ -n "$DISK" ]; then
    mkdir -p /system/tmp/mnt
    if mount "$DISK" /system/tmp/mnt 2>/dev/null; then
        if [ -d /system/tmp/mnt/persistent ]; then
            mount --bind /system/tmp/mnt/persistent /system/state/persistent 2>/dev/null || true
        fi
        if [ -d /system/tmp/mnt/packages ]; then
            mount --bind /system/tmp/mnt/packages /system/packages 2>/dev/null || true
        fi
    else
        mkfs.ext2 -q "$DISK" 2>/dev/null || true
        if mount "$DISK" /system/tmp/mnt 2>/dev/null; then
            mkdir -p /system/tmp/mnt/{persistent,packages,cache,builds}
            cp -a /system/packages/* /system/tmp/mnt/packages/ 2>/dev/null || true
            cp -a /system/state/persistent/* /system/tmp/mnt/persistent/ 2>/dev/null || true
            mount --bind /system/tmp/mnt/persistent /system/state/persistent 2>/dev/null || true
            mount --bind /system/tmp/mnt/packages /system/packages 2>/dev/null || true
        fi
    fi
fi

# ── Phase 7: Networking ──────────────────────────────────────
ip link set lo up 2>/dev/null || true
for nic in eth0 ens3 enp0s3; do
    if ip link show "$nic" >/dev/null 2>&1; then
        ip link set "$nic" up 2>/dev/null || true
        udhcpc -i "$nic" -q -n 2>/dev/null && break
    fi
done

# ── Phase 8: Banner ──────────────────────────────────────────
echo ""
echo "  ____  _                        "
echo " | __ )(_)_ __   __ _ _   ___  __"
echo " |  _ \| | '_ \ / _\` | | | \ \/ /"
echo " | |_) | | | | | (_| | |_| |>  < "
echo " |____/|_|_| |_|\__, |\__,_/_/\_\\"
echo "                |___/             "
echo ""
echo "  Bingux v2 — Self-Hosting Linux"
echo ""
PKGS=$(ls /system/packages/ 2>/dev/null | wc -l)
BINS=$(ls /system/profiles/current/bin/ 2>/dev/null | wc -l)
echo "  Packages: $PKGS"
echo "  Binaries: $BINS"
echo "  Kernel:   $(uname -r)"
echo ""

# ── Phase 9: Boot mode ──────────────────────────────────────
SYSTEMD_BIN="/system/packages/systemd-src-256.11-x86_64-linux/lib/systemd/systemd"

if grep -q "init.systemd" /system/kernel/proc/cmdline 2>/dev/null && [ -f "$SYSTEMD_BIN" ]; then
    echo "[init] Handing off to systemd..."
    mkdir -p /system/kernel/sys/fs/cgroup
    mount -t cgroup2 cgroup2 /system/kernel/sys/fs/cgroup 2>/dev/null || true

    SYSD="/system/packages/systemd-src-256.11-x86_64-linux"
    mkdir -p /usr/bin /usr/sbin /usr/lib/systemd
    for b in systemd-tmpfiles udevadm journalctl systemctl busctl; do
        [ -f "$SYSD/bin/$b" ] && ln -sf "$SYSD/bin/$b" "/usr/bin/$b" 2>/dev/null
    done
    for d in systemd-journald systemd-udevd systemd-executor systemd-shutdown; do
        [ -f "$SYSD/lib/systemd/$d" ] && ln -sf "$SYSD/lib/systemd/$d" "/usr/lib/systemd/$d" 2>/dev/null
    done
    exec "$SYSTEMD_BIN" --system --unit=multi-user.target
else
    echo "[init] Interactive shell"
    echo "  bingux-gcc  — compile C"
    echo "  bsys build  — build packages"
    echo "  bsys apply  — recompose system profile"
    echo "  bpkg list   — list packages"
    echo "  bxc run     — run in sandbox"
    echo ""
    exec /bin/sh -l
fi
