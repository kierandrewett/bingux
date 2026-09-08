#!/usr/bin/env bash
set -euo pipefail
export PATH="$PATH"
repo_dir="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
cp "$repo_dir"/shell/bingux/*.qml "$repo_dir"/shell/bingux/*.js "$repo_dir"/shell/bingux/*.py "$repo_dir/shell/bingux/qmldir" "$test_dir/"
cp -R "$repo_dir/shell/bingux/preview-assets" "$test_dir/"
cp "$repo_dir/tests/document-preview.qml" "$test_dir/shell.qml"
sed -i 's@return runtimeDirectory ? runtimeDirectory + "/bingux/search-v1.sock" : "";@return "";@' "$test_dir/SearchSocket.qml"
export BINGUX_PREVIEW_FIXTURES="$test_dir"
python3 - <<'PY'
import cairo
import os
from pathlib import Path
path = Path(os.environ["BINGUX_PREVIEW_FIXTURES"])
surface = cairo.PDFSurface(str(path / "document.pdf"), 210, 297)
context = cairo.Context(surface)
for colour in [(0.2, 0.4, 0.8), (0.8, 0.4, 0.2)]:
    context.set_source_rgb(1, 1, 1)
    context.paint()
    context.set_source_rgb(*colour)
    context.rectangle(20, 20, 170, 40)
    context.fill()
    context.set_source_rgb(0.15, 0.15, 0.15)
    context.select_font_face("Sans")
    context.set_font_size(16)
    context.move_to(20, 90)
    context.show_text("Document preview")
    context.set_font_size(9)
    for index in range(8):
        context.move_to(20, 115 + index * 16)
        context.show_text("Read, scroll and zoom without opening the file.")
    context.show_page()
surface.finish()
image = cairo.ImageSurface(cairo.FORMAT_ARGB32, 480, 240)
context = cairo.Context(image)
context.set_source_rgb(0.15, 0.5, 0.35)
context.paint()
context.set_source_rgb(0.9, 0.7, 0.25)
context.arc(350, 70, 40, 0, 6.283)
context.fill()
image.write_to_png(str(path / "photo.png"))
(path / "note.txt").write_text("A plain text preview.\n")
(path / "notes.md").write_text("---\ntitle: Preview notes\ntags: [desktop, design]\n" + "".join(f"field{i}: Value {i}\n" for i in range(12)) + "---\n# Markdown heading\n\n**Formatted** content.")
(path / "table.csv").write_text('Name,Count\n"First, item",12\nSecond,34\n')
(path / "message.eml").write_text('From: sender@example.test\nSubject: Preview message\nContent-Type: text/plain; charset=utf-8\n\nA readable message.')
import zipfile
with zipfile.ZipFile(path / "archive.zip", 'w') as archive:
    archive.writestr('notes/readme.md', '# Read me')
(path / "page.html").write_text("<h1>HTML heading</h1><p>Readable <b>content</b>.</p>")
import sqlite3
connection = sqlite3.connect(path / "data.sqlite")
connection.executescript("CREATE TABLE entries (id INTEGER, value TEXT); INSERT INTO entries VALUES (1, 'First row'); CREATE TABLE other (value TEXT); INSERT INTO other VALUES ('Second table');")
connection.close()
from PIL import Image
frames = [Image.new("RGB", (120, 80), colour) for colour in ["red", "blue"]]
frames[0].save(path / "animated.gif", save_all=True, append_images=frames[1:], duration=150, loop=0)
from docx import Document
document = Document()
document.add_heading("Office preview", 0)
document.add_paragraph("Rendered document content.")
document.save(path / "office.docx")
import subprocess
subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=green:s=160x90:d=10", "-c:v", "mpeg4", "-y", str(path / "clip.mp4")], check=True)
PY
BINGUX_ICON_HELPER='' BINGUX_PREVIEW_HELPER='' NO_AT_BRIDGE=1 timeout 40s "${QUICKSHELL_BIN:-quickshell}" -p "$test_dir" --no-color > "$test_dir/runtime.log" 2>&1 || { cat "$test_dir/runtime.log"; exit 1; }
grep -q 'DOCUMENT_PREVIEW_PASS' "$test_dir/runtime.log" || { cat "$test_dir/runtime.log"; exit 1; }
if grep -E 'TypeError|ReferenceError|Unable to assign|Binding loop' "$test_dir/runtime.log"; then exit 1; fi
echo "PASS: Right Arrow preview, fixed search layout, PDF rendering, zoom, scroll, pan, file switching and cleanup"
