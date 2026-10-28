#!/usr/bin/env python3
"""Builds the fixture home used for the file chooser captures."""
import os, shutil, struct, zlib, math, sys, time
args = [a for a in sys.argv[1:] if not a.startswith("--")]
root = args[0] if args else "/tmp/bingux-files-home"
# --link-downloads: ~/Downloads is a symlink to a folder outside the home, as
# when it points at a network share. The chooser must still draw the Downloads
# icon on the link, and a plain folder for the target reached by its real path.
link_downloads = "--link-downloads" in sys.argv
if os.path.isdir(root):
    # The no-permission folder cannot be walked until it is readable again.
    if os.path.isdir(f"{root}/Private"):
        os.chmod(f"{root}/Private", 0o755)
    shutil.rmtree(root)
for d in ["Desktop","Documents","Music","Pictures","Videos","Projects","Empty","Private","Documents/Taxes","Templates","Public"]:
    os.makedirs(f"{root}/{d}")
if link_downloads:
    target = root.rstrip("/") + "-shares/downloads"
    shutil.rmtree(os.path.dirname(target), ignore_errors=True)
    os.makedirs(target)
    os.symlink(target, f"{root}/Downloads")
else:
    os.makedirs(f"{root}/Downloads")
# Many sibling folders for the crumb dropdown, and a deep trail for the overflow.
for i in range(1, 61):
    os.makedirs(f"{root}/Projects/{['alpha','bravo','charlie','delta','echo','foxtrot'][i % 6]}-{i:02}")
deep = root + "/Deep"
for level in ["north-america", "united-states", "california", "san-francisco", "mission-district", "valencia-street", "apartment-files", "tenancy", "2026", "correspondence", "landlord", "repairs", "kitchen", "photos"]:
    deep += "/" + level
os.makedirs(deep)
open(deep + "/Invoice with a very long name for the middle ellipsis 2026-10-24.pdf", "wb").write(b"\0" * 90_000)

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

# Real content for the files Quick Look shows as text, so previews are not
# blocks of NUL bytes. Times are kept.
def write_text(rel, body):
    p = f"{root}/{rel}"
    st = os.stat(p)
    with open(p, "w") as f: f.write(body)
    os.utime(p, (st.st_atime, st.st_mtime))
write_text("Documents/Meeting notes.txt", "Meeting notes, 22 October\n\nAttendees: Ana, Bilal, Chen, Dara\n\n1. Roadmap\n   - The files app ships with Quick Look and a proper Properties dialog.\n   - Settings gets the new network pages next.\n\n2. Open questions\n   - Should Space also preview inside the file chooser? Yes: same view.\n   - How do we show a folder that holds a million files? Count it live.\n\nActions\n   Ana: write the docs\n   Bilal: check the phone layout\n   Chen: thumbnails for PDFs\n")
write_text("Documents/todo.md", "# To do\n\n- [x] Quick Look on Space\n- [x] Properties with permissions\n- [ ] Rename inside Properties\n- [ ] Default application chooser\n\n## Later\n\nSee `docs/bingux-ui.md` for the keys.\n")
write_text("Downloads/wg0.conf", "[Interface]\nPrivateKey = (not a real key)\nAddress = 10.0.0.2/32\nDNS = 10.0.0.1\n\n[Peer]\nPublicKey = (not a real key)\nEndpoint = vpn.example.org:51820\nAllowedIPs = 0.0.0.0/0\n")
write_text("Projects/main.rs", "use std::collections::HashMap;\n\n/// Count how often each word appears.\nfn count(text: &str) -> HashMap<&str, usize> {\n    let mut counts = HashMap::new();\n    for word in text.split_whitespace() {\n        *counts.entry(word).or_insert(0) += 1;\n    }\n    counts\n}\n\nfn main() {\n    let counts = count(\"the quick brown fox jumps over the lazy dog the end\");\n    let mut words: Vec<_> = counts.into_iter().collect();\n    words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));\n    for (word, n) in words {\n        println!(\"{n:>3} {word}\");\n    }\n}\n")
write_text("Downloads/install.sh", "#!/bin/sh\nset -eu\necho \"Installing\"\n")
os.chmod(f"{root}/Downloads/install.sh", 0o755)
# A long log, to show the "first 64 KiB" notice, and some JSON.
os.makedirs(f"{root}/Notes")
with open(f"{root}/Notes/server.log", "w") as f:
    for n in range(1, 6001):
        f.write(f"2026-10-24T12:{n // 100 % 60:02}:{n % 60:02}Z INFO request {n} served in {n % 97} ms\n")
with open(f"{root}/Notes/settings.json", "w") as f:
    f.write('{\n  "theme": "dark",\n  "accent": "blue",\n  "files": {"view": "grid", "zoom": 96, "hidden": false},\n  "recent": ["Aurora.png", "Meeting notes.txt"]\n}\n')
with open(f"{root}/Notes/firmware.dat", "wb") as f:
    f.write(bytes((n * 37 + 11) % 256 for n in range(5000)))
with open(f"{root}/Notes/README", "w") as f:
    f.write("A file with no extension, still plain text.\nQuick Look reads the head of an unknown type and shows it as text.\n")
# A photograph with EXIF, if Pillow is installed (the capture does not need it).
try:
    from PIL import Image
    from PIL.TiffImagePlugin import IFDRational
    img = Image.new("RGB", (1600, 1200))
    px = img.load()
    for y in range(1200):
        for x in range(1600):
            px[x, y] = (int(40 + 150 * x / 1600), int(90 + 100 * y / 1200), int(200 - 120 * x / 1600))
    exif = img.getexif()
    exif[0x010F] = "Canon"
    exif[0x0110] = "Canon EOS R5"
    exif[0x0112] = 1
    sub = exif.get_ifd(0x8769)
    sub[0x9003] = "2026:10:12 17:42:09"
    sub[0x829A] = IFDRational(1, 250)
    sub[0x829D] = IFDRational(28, 10)
    sub[0x8827] = 200
    sub[0xA434] = "RF24-70mm F2.8 L IS USM"
    img.save(f"{root}/Pictures/Camera.jpg", "JPEG", quality=88, exif=exif)
except ImportError:
    pass
# The preview's own folder: one real file of each kind the viewers show.
media = f"{root}/Media"
os.makedirs(media)
png(f"{media}/Gradient.png", 1600, 1000, lambda x, y, w, h: (30 + x * 200 // w, 60 + y * 120 // h, 200 - x * 100 // w))
png(f"{media}/Small.png", 120, 80, lambda x, y, w, h: (220, 60 + x, 60 + y * 2))
open(f"{media}/Logo.svg", "w").write('<svg xmlns="http://www.w3.org/2000/svg" width="240" height="160" viewBox="0 0 240 160"><rect width="240" height="160" fill="#e0e8ff"/><circle cx="120" cy="80" r="50" fill="#2a6fdb"/></svg>\n')
open(f"{media}/Note UTF-8.txt", "w", encoding="utf-8").write("Héllo, wörld\nA second line with an arrow →\n")
open(f"{media}/Note Latin-1.txt", "wb").write("Café crème\nA file that is not UTF-8\n".encode("latin-1"))
open(f"{media}/Note UTF-16.txt", "wb").write(b"\xff\xfe" + "UTF-16 text with a byte order mark\n".encode("utf-16-le"))
with open(f"{media}/Blob.bin", "wb") as f:
    f.write(bytes((n * 73 + 5) % 256 for n in range(4096)))
import tarfile, zipfile, io
with zipfile.ZipFile(f"{media}/Bundle.zip", "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("readme.txt", "Hello\n" * 20)
    z.writestr("src/main.rs", "fn main() {}\n")
    z.writestr("src/lib.rs", "pub fn f() {}\n" * 8)
    z.writestr("assets/", "")
    z.writestr("assets/logo.svg", "<svg/>")
with tarfile.open(f"{media}/Backup.tar.gz", "w:gz") as t:
    for name, body in [("notes/a.txt", b"alpha\n" * 30), ("notes/b.txt", b"bravo\n" * 10), ("photo.raw", bytes(range(256)) * 40)]:
        info = tarfile.TarInfo(name)
        info.size = len(body)
        t.addfile(info, io.BytesIO(body))
def make_pdf(path, pages):
    """A small PDF in Helvetica: `pages` is a list of lists of text lines."""
    objects = []
    def add(body):
        objects.append(body)
        return len(objects)
    font = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    kids = []
    page_ids = []
    pages_id = len(objects) + 1 + 2 * len(pages)
    for lines in pages:
        text = "BT /F1 12 Tf 50 780 Td 16 TL " + " ".join(f"({line.replace('(', '[').replace(')', ']')}) Tj T*" for line in lines) + " ET"
        stream = add(f"<< /Length {len(text)} >>\nstream\n{text}\nendstream".encode())
        page = add(f"<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 595 842] /Contents {stream} 0 R /Resources << /Font << /F1 {font} 0 R >> >> >>".encode())
        kids.append(f"{page} 0 R")
    add(f"<< /Type /Pages /Kids [{' '.join(kids)}] /Count {len(pages)} >>".encode())
    catalog = add(f"<< /Type /Catalog /Pages {pages_id} 0 R >>".encode())
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for number, body in enumerate(objects, 1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n".encode() + body + b"\nendobj\n"
    xref = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for offset in offsets:
        out += f"{offset:010} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objects) + 1} /Root {catalog} 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    open(path, "wb").write(out)
make_pdf(f"{media}/Report.pdf", [
    [f"Quarterly report, page {page}"] + [f"Line {n}: the quick brown fox jumps over the lazy dog, invoice {page * 100 + n}" for n in range(1, 41)]
    for page in range(1, 4)
])
# A film and a song, when ffmpeg is installed: the player is shown on real media.
import subprocess
if shutil.which("ffmpeg"):
    quiet = ["ffmpeg", "-loglevel", "error", "-y"]
    subprocess.run(quiet + ["-f", "lavfi", "-i", "testsrc2=size=640x360:rate=30:duration=6", "-f", "lavfi", "-i", "sine=frequency=440:duration=6", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", f"{media}/Film.mp4"], check=False)
    subprocess.run(quiet + ["-f", "lavfi", "-i", "sine=frequency=330:duration=8", "-c:a", "libopus", f"{media}/Song.opus"], check=False)
    open(f"{media}/Film.srt", "w").write("1\n00:00:00,500 --> 00:00:02,500\nHello subtitle\n\n2\n00:00:03,000 --> 00:00:05,000\nSecond line\n")
os.makedirs(f"{root}/.config/gtk-3.0")
open(f"{root}/.config/user-dirs.dirs", "w").write(
 f'XDG_DESKTOP_DIR="$HOME/Desktop"\nXDG_DOWNLOAD_DIR="$HOME/Downloads"\nXDG_DOCUMENTS_DIR="$HOME/Documents"\nXDG_MUSIC_DIR="$HOME/Music"\nXDG_PICTURES_DIR="$HOME/Pictures"\nXDG_VIDEOS_DIR="$HOME/Videos"\nXDG_TEMPLATES_DIR="$HOME/Templates"\nXDG_PUBLICSHARE_DIR="$HOME/Public"\n')
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
