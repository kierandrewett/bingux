#!/usr/bin/env python3
"""Read local document metadata and render individual preview pages in memory."""
import base64
import io
import json
import math
from datetime import datetime, timezone
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import preview_formats as formats
import preview_extra as extra
from preview_limits import MAX_FILE_BYTES, check_file

import cairo
from PIL import Image, ImageCms
import gi

gi.require_version("GdkPixbuf", "2.0")
gi.require_version("Poppler", "0.18")
from gi.repository import GdkPixbuf, Gio, Poppler

MAX_TEXT_BYTES = 256 * 1024
TEXT_SUFFIXES = {".md", ".markdown", ".jsx", ".tsx", ".cpp", ".hpp", ".go", ".rb", ".java", ".kt", ".swift", ".bash", ".zsh", ".scss", ".sql", ".htm", ".txt", ".log", ".csv", ".json", ".yaml", ".yml", ".toml", ".ini", ".conf", ".xml", ".html", ".css", ".js", ".ts", ".py", ".rs", ".c", ".h", ".sh", ".nix", ".qml"}


def inspect(path):
    check_file(path)
    suffix = path.suffix.lower()
    if suffix in extra.EXTRA_SUFFIXES:
        return "extra", None
    if suffix in formats.DATABASE_SUFFIXES:
        return "database", None
    if suffix in formats.MEDIA_SUFFIXES:
        return "media", None
    info = Gio.File.new_for_path(str(path)).query_info("standard::content-type", Gio.FileQueryInfoFlags.NONE, None)
    mime = info.get_content_type() or ""
    if suffix in formats.OFFICE_SUFFIXES:
        return "pdf", Poppler.Document.new_from_file(formats.office_pdf(path).as_uri(), None)
    if mime == "application/pdf":
        return "pdf", Poppler.Document.new_from_file(path.as_uri(), None)
    if mime.startswith("image/"):
        details = GdkPixbuf.Pixbuf.get_file_info(str(path))
        if details[0] is None:
            raise ValueError("No preview is available for this image format.")
        return "image", details
    if mime.startswith("text/") or path.suffix.lower() in TEXT_SUFFIXES:
        return "text", None
    raise ValueError("No preview is available for this file format.")


def image_metadata(path):
    """Read embedded fields without inventing absent dates or profiles."""
    result = {}
    try:
        with Image.open(path) as image:
            result["colourSpace"] = {"L": "Greyscale", "LA": "Greyscale with alpha", "RGBA": "RGB with alpha", "P": "Indexed colour"}.get(image.mode, image.mode)
            dpi = image.info.get("dpi")
            if dpi and len(dpi) == 2:
                result["resolution"] = f"{dpi[0]:.0f} × {dpi[1]:.0f} dpi"
            exif = image.getexif().get_ifd(34665)
            captured = exif.get(36867)
            if captured:
                try:
                    result["contentCreated"] = datetime.strptime(captured, "%Y:%m:%d %H:%M:%S").isoformat()
                except ValueError:
                    pass
            profile = image.info.get("icc_profile")
            if profile:
                try:
                    result["colourProfile"] = ImageCms.getProfileDescription(ImageCms.ImageCmsProfile(io.BytesIO(profile))).strip()
                except (OSError, ValueError):
                    pass
    except (OSError, ValueError):
        pass  # GdkPixbuf may support a format that Pillow does not.
    return result


def metadata(path):
    kind, document = inspect(path)
    stat = path.stat()
    info = Gio.File.new_for_path(str(path)).query_info("standard::content-type,time::created", Gio.FileQueryInfoFlags.NONE, None)
    mime = info.get_content_type()
    common = {"file": {
        "sizeBytes": stat.st_size,
        "modified": datetime.fromtimestamp(stat.st_mtime, timezone.utc).isoformat(),
        "type": Gio.content_type_get_description(mime),
    }}
    if info.has_attribute("time::created") and info.get_attribute_uint64("time::created") > 0:
        common["file"]["created"] = datetime.fromtimestamp(info.get_attribute_uint64("time::created"), timezone.utc).isoformat()
    if kind == "extra":
        return common | extra.preview(path, formats.formatted_text)
    if kind == "database":
        return common | formats.database(path)
    if kind == "media":
        return common | formats.media(path)
    if kind == "pdf":
        pages = []
        for index in range(document.get_n_pages()):
            width, height = document.get_page(index).get_size()
            pages.append({"width": width, "height": height})
        return common | {"kind": kind, "pages": pages}
    if kind == "image":
        width, height = document[1], document[2]
        sample = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), 16, 16, True)
        if sample.get_option("orientation") in ("5", "6", "7", "8"):
            width, height = height, width
        result = common | {"kind": kind, "pages": [{"width": width, "height": height}], "image": image_metadata(path)}
        if path.suffix.lower() == ".gif":
            result.update(kind="animation", source=path.as_uri())
        return result
    with path.open("rb") as stream:
        data = stream.read(MAX_TEXT_BYTES + 1)
    if b"\x00" in data:
        raise ValueError("No text preview is available for this file.")
    result = common | {"kind": kind, "text": data[:MAX_TEXT_BYTES].decode("utf-8", errors="replace"), "truncated": len(data) > MAX_TEXT_BYTES, "format": "markdown" if path.suffix.lower() in {".md", ".markdown"} else "html" if path.suffix.lower() in {".html", ".htm"} else "plain"}
    if result["format"] == "markdown":
        result["text"], result["frontmatter"], result["frontmatterWarning"] = extra.frontmatter(result["text"])
    if result["format"] == "plain" and path.suffix.lower() == ".json" and not result["truncated"]:
        try: result["text"] = json.dumps(json.loads(result["text"]), indent=2, ensure_ascii=False)
        except ValueError: pass
    if result["format"] != "plain":
        result["renderedText"] = formats.formatted_text(result["text"], result["format"], path)
    return result


def render(path, page_number, requested_width):
    kind, document = inspect(path)
    width = max(64, min(2400, requested_width))
    if kind == "pdf":
        if page_number < 0 or page_number >= document.get_n_pages():
            raise ValueError("This page is not available.")
        page = document.get_page(page_number)
        page_width, page_height = page.get_size()
        if page_width <= 0 or page_height <= 0:
            raise ValueError("This page has no visible area.")
        scale = min(width / page_width, math.sqrt(8_000_000 / (page_width * page_height)))
        surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, max(1, math.ceil(page_width * scale)), max(1, math.ceil(page_height * scale)))
        context = cairo.Context(surface)
        context.set_source_rgb(1, 1, 1)
        context.paint()
        context.scale(scale, scale)
        page.render(context)
        output = io.BytesIO()
        surface.write_to_png(output)
        pixels = output.getvalue()
    elif kind == "image":
        image = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), width, min(4096, 8_000_000 // width), True)
        image = image.apply_embedded_orientation()
        success, pixels = image.save_to_bufferv("png", [], [])
        if not success:
            raise ValueError("The image could not be rendered.")
    else:
        raise ValueError("This document has no image pages.")
    return {"image": "data:image/png;base64," + base64.b64encode(pixels).decode("ascii")}


def main():
    if sys.argv[1:] == ['serve']:
        import asyncio
        from preview_service import serve
        asyncio.run(serve([sys.executable, str(Path(__file__).resolve())]))
        return
    try:
        mode, filename, *args = sys.argv[1:]
        path = Path(filename)
        if mode == "warm":
            info = metadata(path)
            pages = []
            if info['kind'] in ('pdf', 'image', 'animation'):
                pages = [render(path, page, int(args[0])) for page in range(min(2, len(info['pages'])))]
                if info['kind'] == 'animation' and pages:
                    info['poster'] = pages[0]['image']
            elif info['kind'] == 'video':
                try:
                    info['poster'] = formats.video_poster(path)
                except (ValueError, OSError):
                    pass
            result = {'info': info, 'pages': pages}
        elif mode == "info":
            result = metadata(path)
        elif mode == "table":
            result = formats.database(path, args[0] if args else None, int(args[1]) if len(args) > 1 else 0)
        elif mode == "page":
            result = render(path, int(args[0]), int(args[1]))
        else:
            raise ValueError("Unknown preview request.")
        print(json.dumps(result), flush=True)
    except Exception as error:
        print(json.dumps({"error": str(error)}), flush=True)


if __name__ == "__main__":
    main()
