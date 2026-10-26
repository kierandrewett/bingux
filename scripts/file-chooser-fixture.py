#!/usr/bin/env python3
"""Builds the fixture home used for the file chooser captures."""
import os, shutil, struct, zlib, math, sys, time
root = sys.argv[1] if len(sys.argv) > 1 else "/tmp/bingux-files-home"
if os.path.isdir(root):
    for dirpath, dirs, files in os.walk(root):
        os.chmod(dirpath, 0o755)
    shutil.rmtree(root)
for d in ["Desktop","Documents","Downloads","Music","Pictures","Videos","Projects","Empty","Private","Documents/Taxes"]:
    os.makedirs(f"{root}/{d}")

def png(path, w, h, fn):
    raw = b"".join(b"\0" + b"".join(bytes(fn(x, y, w, h)) for x in range(w)) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

def hsv(h, s, v):
    import colorsys
    r, g, b = colorsys.hsv_to_rgb(h % 1.0, s, v)
    return (int(r*255), int(g*255), int(b*255))

names = ["Aurora","Bamboo","Canyon","Dunes","Evergreen","Fjord","Glacier","Harbor","Island","Jungle","Kelp","Lagoon","Meadow","Nebula","Orchard","Prairie","Quartz","Reef","Sunset","Tundra"]
for i, n in enumerate(names):
    base = i / len(names)
    def fn(x, y, w, h, base=base, i=i):
        t = y / h
        r = math.sin((x / w) * 3 + i) * 0.5 + 0.5
        return hsv(base + 0.12 * r, 0.55 + 0.3 * t, 0.95 - 0.5 * t)
    png(f"{root}/Pictures/{n}.png", 320, 200, fn)
png(f"{root}/Pictures/Screenshot 2026-10-24.png", 320, 200, lambda x,y,w,h: (30+x*200//w, 40, 90+y*120//h))
def stamp(p, days_ago, size=None, text="x"):
    if size is not None:
        with open(p, "wb") as f: f.write(b"\0" * size)
    t = time.time() - days_ago * 86400
    os.utime(p, (t, t))
files = {
 "Documents/Quarterly report.pdf": (3, 2_400_000), "Documents/Meeting notes.txt": (0.2, 3400), "Documents/Budget 2026.xlsx": (12, 88_000),
 "Documents/Taxes/2025.pdf": (200, 1_100_000), "Documents/Résumé.odt": (45, 41_000), "Documents/todo.md": (1, 900),
 "Downloads/wg0.conf": (2, 410), "Downloads/work.ovpn": (5, 5200), "Downloads/home.ovpn": (9, 4900), "Downloads/notes.conf.bak": (9, 100),
 "Downloads/archive.zip": (1, 52_000_000), "Downloads/install.sh": (20, 1500), "Downloads/photo.jpg": (4, 3_100_000),
 "Music/Track 01.mp3": (30, 4_200_000), "Music/Track 02.flac": (30, 31_000_000),
 "Videos/clip.mp4": (7, 120_000_000), "Projects/main.rs": (0.1, 2500), "Projects/Makefile": (0.1, 700),
 "Desktop/shortcut.desktop": (60, 300), "Documents/.hidden-draft.txt": (3, 80),
}
for rel, (days, size) in files.items():
    stamp(f"{root}/{rel}", days, size)
for i in range(1, 41):
    stamp(f"{root}/Documents/Scan {i:03}.pdf", 100 + i, 200_000 + i * 1000)
os.makedirs(f"{root}/.config/gtk-3.0")
open(f"{root}/.config/user-dirs.dirs", "w").write(
 f'XDG_DESKTOP_DIR="$HOME/Desktop"\nXDG_DOWNLOAD_DIR="$HOME/Downloads"\nXDG_DOCUMENTS_DIR="$HOME/Documents"\nXDG_MUSIC_DIR="$HOME/Music"\nXDG_PICTURES_DIR="$HOME/Pictures"\nXDG_VIDEOS_DIR="$HOME/Videos"\n')
open(f"{root}/.config/gtk-3.0/bookmarks", "w").write(f"file://{root}/Projects Work\n")
os.makedirs(f"{root}/.local/share")
iso = lambda d: time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(time.time() - d * 86400))
rec = [("Documents/Quarterly report.pdf", 0.5), ("Pictures/Aurora.png", 1), ("Downloads/wg0.conf", 2), ("Documents/Meeting notes.txt", 3)]
open(f"{root}/.local/share/recently-used.xbel", "w").write(
 '<?xml version="1.0" encoding="UTF-8"?>\n<xbel version="1.0">\n' +
 "".join(f'  <bookmark href="file://{root}/{p}" added="{iso(d)}" modified="{iso(d)}" visited="{iso(d)}"></bookmark>\n' for p, d in rec) + '</xbel>\n')
open(f"{root}/Private/secret.txt", "w").write("x")
os.chmod(f"{root}/Private", 0)
print("fixture at", root)
