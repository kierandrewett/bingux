#!/usr/bin/env python3
# Guest-side boot tracer: logs changes of DRM clients (master flag), CRTC state,
# scanout framebuffer owner, VT and console mode with CLOCK_MONOTONIC stamps.
import fcntl, glob, os, re, sys, time

LOG = "/run/bsm-boot-trace.log"
out = open(LOG, "a", buffering=1)
last = {}
t0 = time.monotonic()

def emit(kind, text):
    out.write("%.4f %s %s\n" % (time.monotonic(), kind, text))

def watch(kind, value):
    if last.get(kind) != value:
        last[kind] = value
        emit(kind, value)

def read(path):
    try:
        with open(path) as f:
            return f.read()
    except OSError:
        return None

def mark():
    # Lets tests/bsm_boot_analysis.py put the kernel log (printk clock) on CLOCK_MONOTONIC.
    try:
        with open("/dev/kmsg", "w") as k:
            k.write("bsm-boot-trace: mono=%.6f\n" % time.monotonic())
    except OSError:
        pass

emit("start", "tracer up")
mark()
try:
    tty = os.open("/dev/tty0", os.O_RDWR | os.O_NOCTTY)
except OSError:
    tty = None
dirs = []
tick = 0
while True:
    if tick % 500 == 0:
        mark()
    if not dirs:
        dirs = [os.path.dirname(p) for p in glob.glob("/sys/kernel/debug/dri/*/clients")]
    for d in dirs:
        name = os.path.basename(d)
        text = read(d + "/clients")
        if text is not None:
            rows = []
            for line in text.splitlines()[1:]:
                p = line.split()
                if len(p) >= 4:
                    rows.append("%s:%s:%s" % (p[0], p[1], "M" if p[3] == "y" else "-"))
            watch("clients-" + name, " ".join(sorted(rows)))
        if tick % 2 == 0:
            st = read(d + "/state")
            if st is not None:
                crtcs, planes, cur = [], [], None
                for line in st.splitlines():
                    m = re.match(r"(crtc|plane)\[(\d+)\]: (\S+)", line)
                    if m:
                        cur = [m.group(1), m.group(3), {}]
                        (crtcs if m.group(1) == "crtc" else planes).append(cur)
                        continue
                    if cur is None:
                        continue
                    s = line.strip()
                    for key in ("enable", "active", "crtc", "fb", "mode_changed", "active_changed", "connectors_changed"):
                        if s.startswith(key + "="):
                            cur[2][key] = s.split("=", 1)[1]
                    if s.startswith("mode:"):
                        cur[2]["mode"] = s.split('"')[1] if '"' in s else s
                    if s.startswith("allocated by ="):
                        cur[2]["by"] = s.split("=", 1)[1].strip()
                for c in crtcs:
                    v = c[2]
                    watch("crtc-%s" % c[1], "en=%s act=%s mode=%s" % (v.get("enable"), v.get("active"), v.get("mode")))
                for pl in planes:
                    v = pl[2]
                    if v.get("crtc", "(null)") != "(null)":
                        watch("plane-%s" % pl[1], "crtc=%s by=%s" % (v.get("crtc"), v.get("by")))
                        # The framebuffer id changes on every flip; its owner (by=) is what matters.
                    else:
                        watch("plane-%s" % pl[1], "off")
    watch("vt", (read("/sys/class/tty/tty0/active") or "").strip())
    if tty is not None:
        try:
            buf = bytearray(4)
            fcntl.ioctl(tty, 0x4B3B, buf)
            watch("kd", "graphics" if buf[0] == 1 else "text")
        except OSError:
            pass
        try:
            mode = bytearray(8)
            fcntl.ioctl(tty, 0x5601, mode)
            watch("vtmode", {0: "auto", 1: "process", 2: "ackacq"}.get(mode[0], str(mode[0])))
        except OSError:
            pass
    watch("plymouth", "yes" if os.path.exists("/run/plymouth/pid") else "no")
    tick += 1
    time.sleep(0.01)
