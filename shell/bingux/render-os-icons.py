#!/usr/bin/env python3
"""Resolve OS icons and render SVGs in memory with librsvg, not QtSvg."""

import base64
from collections import OrderedDict, deque
import json
import os
from pathlib import Path
import sys
from urllib.parse import parse_qs, unquote, urlparse

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf, Gio, GLib, Gtk  # noqa: E402 - Select GI versions before importing their modules.

try:
    gi.require_version("GnomeDesktop", "3.0")
    from gi.repository import GnomeDesktop
except (ValueError, ImportError):
    GnomeDesktop = None

FILE_PREVIEW_PREFIX = "file-preview:"
SIZE = 192  # Covers the search icon's 4x launch animation and HiDPI rows.
FILE_ATTRIBUTES = (
    "standard::icon,standard::content-type,standard::type,time::modified,"
    "thumbnail::path,thumbnail::is-valid,metadata::custom-icon"
)


def file_preview(source, theme):
    path = Path(unquote(source[len(FILE_PREVIEW_PREFIX) :]))
    if not path.is_absolute():
        raise ValueError("file preview requires an absolute path")
    file = Gio.File.new_for_path(str(path))
    info = file.query_info(FILE_ATTRIBUTES, Gio.FileQueryInfoFlags.NONE, None)
    custom = info.get_attribute_string("metadata::custom-icon")
    if custom:
        parsed = urlparse(custom)
        candidate = Path(unquote(parsed.path)) if parsed.scheme == "file" else path / custom
        if parsed.scheme in ("", "file") and candidate.is_file():
            return candidate, file, info
    thumbnail = info.get_attribute_byte_string("thumbnail::path")
    if thumbnail and info.get_attribute_boolean("thumbnail::is-valid") and Path(thumbnail).is_file():
        return Path(thumbnail), file, info
    icon = theme.lookup_by_gicon(info.get_icon(), SIZE, Gtk.IconLookupFlags.FORCE_SIZE)
    if icon is None:
        icon = theme.lookup_icon("text-x-generic", SIZE, Gtk.IconLookupFlags.FORCE_SIZE)
    return Path(icon.get_filename()), file, info


class FileThumbnails:
    """Use GNOME's thumbnailers off the UI thread, with two bounded jobs."""

    def __init__(self):
        self.factory = (
            GnomeDesktop.DesktopThumbnailFactory.new(GnomeDesktop.DesktopThumbnailSize.LARGE) if GnomeDesktop else None
        )
        self.pending = set()
        self.queue = deque()
        self.active = 0

    def request(self, source, file, info):
        if not self.factory or source in self.pending or len(self.pending) >= 40:
            return
        if info.get_file_type() != Gio.FileType.REGULAR or info.get_attribute_string("metadata::custom-icon"):
            return
        uri = file.get_uri()
        mtime = info.get_attribute_uint64("time::modified")
        cached = self.factory.lookup(uri, mtime)
        if cached:
            emit({"source": source, "resolved": Path(cached).as_uri()})
            return
        mime = info.get_content_type()
        if not mime or not self.factory.can_thumbnail(uri, mime, mtime):
            return
        self.pending.add(source)
        self.queue.append((source, uri, mime, mtime))
        self.start_next()

    def start_next(self):
        while self.queue and self.active < 2:
            source, uri, mime, mtime = self.queue.popleft()
            self.active += 1
            cancel = Gio.Cancellable()
            timeout = GLib.timeout_add_seconds(5, lambda cancellable=cancel: (cancellable.cancel(), False)[1])
            self.factory.generate_thumbnail_async(uri, mime, cancel, self.generated, (source, uri, mtime, timeout))

    def generated(self, factory, result, request):
        source, uri, mtime, timeout = request
        # The timeout may already have fired.
        if GLib.MainContext.default().find_source_by_id(timeout):
            GLib.source_remove(timeout)
        try:
            pixbuf = factory.generate_thumbnail_finish(result)
            if pixbuf:
                emit({"source": source, "resolved": pixbuf_uri(pixbuf)})
                factory.save_thumbnail_async(pixbuf, uri, mtime, None, self.saved, None)
        except (GLib.Error, ValueError):
            pass  # Keep the file type icon when no preview can be generated.
        finally:
            self.pending.discard(source)
            self.active -= 1
            self.start_next()

    @staticmethod
    def saved(factory, result, _data):
        try:
            factory.save_thumbnail_finish(result)
        except GLib.Error:
            pass


def pixbuf_uri(pixbuf):
    success, pixels = pixbuf.save_to_bufferv("png", [], [])
    if not success:
        raise ValueError("could not encode rendered icon")
    return "data:image/png;base64," + base64.b64encode(pixels).decode("ascii")


def sample_palette(source, theme, cache):
    path = icon_path(source, theme).resolve(strict=True)
    stat = path.stat()
    key = (str(path), stat.st_mtime_ns, stat.st_size, "palette")
    cached = cache.get(key)
    if cached is not None:
        return json.loads(cached)
    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), 32, 32, True)
    pixels, channels, stride = pixbuf.get_pixels(), pixbuf.get_n_channels(), pixbuf.get_rowstride()
    buckets = {}
    for y in range(pixbuf.get_height()):
        for x in range(pixbuf.get_width()):
            offset = y * stride + x * channels
            if channels == 4 and pixels[offset + 3] < 128:
                continue
            rgb = pixels[offset : offset + 3]
            bucket = buckets.setdefault(tuple(value // 32 for value in rgb), [0, 0, 0, 0])
            for channel in range(3):
                bucket[channel] += rgb[channel]
            bucket[3] += 1
    palette = [
        "#%02x%02x%02x" % tuple(round(value / bucket[3]) for value in bucket[:3])
        for bucket in sorted(buckets.values(), key=lambda item: item[3], reverse=True)[:8]
    ]
    cache.put(key, json.dumps(palette))
    return palette


class RenderCache:
    """Bounded in-process LRU; no PNG files or disk cache."""

    def __init__(self, limit=8 * 1024 * 1024):
        self.limit = limit
        self.bytes = 0
        self.entries = OrderedDict()

    def get(self, key):
        if key not in self.entries:
            return None
        self.entries.move_to_end(key)
        return self.entries[key]

    def put(self, key, value):
        if key in self.entries:
            self.bytes -= len(self.entries.pop(key))
        if len(value) > self.limit:
            return
        while self.entries and self.bytes + len(value) > self.limit:
            self.bytes -= len(self.entries.popitem(last=False)[1])
        self.entries[key] = value
        self.bytes += len(value)


def requested_size(source):
    try:
        return max(64, min(1024, int(parse_qs(urlparse(source).query).get("bingux-size", [SIZE])[0])))
    except (TypeError, ValueError):
        return SIZE


def icon_path(source, theme):
    if source.startswith(FILE_PREVIEW_PREFIX):
        return file_preview(source, theme)[0]
    if source.startswith("image://icon/"):
        name = unquote(source[len("image://icon/") :].split("?", 1)[0])
        if os.path.isabs(name):
            return Path(name)
        # StatusNotifierItem icons may live in an app's private IconThemePath.
        # Resolve only a basename inside that explicit directory, never a path
        # traversal or a modification to the desktop's global icon theme.
        options = parse_qs(urlparse(source).query)
        directory = options.get("path", [""])[0]
        if os.path.isabs(directory) and name not in (".", "..") and Path(name).name == name:
            for suffix in ("", ".png", ".svg", ".svgz", ".xpm"):
                candidate = Path(directory) / (name + suffix)
                if candidate.is_file():
                    return candidate
        info = theme.lookup_icon(name, requested_size(source), Gtk.IconLookupFlags.FORCE_SIZE)
        # Quickshell.iconPath leaves missing names in the URL and supplies the
        # replacement as a query parameter. Render that replacement here too,
        # so SVG fallbacks keep their alpha instead of going back through QtSvg.
        if info is None:
            fallback = options.get("fallback", ["application-x-executable"])[0]
            info = theme.lookup_icon(fallback, requested_size(source), Gtk.IconLookupFlags.FORCE_SIZE)
        if info is None:
            raise ValueError("icon not found")
        return Path(info.get_filename())
    parsed = urlparse(source)
    if parsed.scheme == "file" and parsed.netloc in ("", "localhost"):
        return Path(unquote(parsed.path))
    if not parsed.scheme and os.path.isabs(source):
        return Path(source)
    raise ValueError("only local OS icons are supported")


def resolve(source, theme, cache):
    path = icon_path(source, theme).resolve(strict=True)
    if path.suffix.lower() not in (".svg", ".svgz"):
        return path.as_uri()
    stat = path.stat()
    size = requested_size(source)
    identity = (str(path), stat.st_mtime_ns, stat.st_size, size)
    cached = cache.get(identity)
    if cached is not None:
        return cached
    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(path), size, size, True)
    # PNG is only the lossless in-memory transport to Qt, never a file.
    image = pixbuf_uri(pixbuf)
    cache.put(identity, image)
    return image


def emit(record):
    print(json.dumps(record), flush=True)


def main():
    theme = Gtk.IconTheme.get_default()
    cache = RenderCache()
    thumbnails = FileThumbnails()
    loop = GLib.MainLoop()
    pending = bytearray()
    theme.connect("changed", lambda *_: emit({"reset": True}))

    def read_request(_stream, _condition):
        chunk = os.read(sys.stdin.fileno(), 65536)
        if not chunk:
            loop.quit()
            return False
        pending.extend(chunk)
        while b"\n" in pending:
            line, _, rest = pending.partition(b"\n")
            pending[:] = rest
            if len(line) > 8192:
                loop.quit()
                return False
            handle_request(line)
        if len(pending) > 8192:
            loop.quit()
            return False
        return True

    def handle_request(line):
        source = ""
        try:
            source = json.loads(line)["source"]
            if not isinstance(source, str) or len(source) > 4096:
                raise ValueError("invalid source")
            if source.startswith(FILE_PREVIEW_PREFIX):
                path, file, info = file_preview(source, theme)
                emit({"source": source, "resolved": resolve(path.as_uri(), theme, cache)})
                thumbnails.request(source, file, info)
            else:
                emit(
                    {
                        "source": source,
                        "resolved": resolve(source, theme, cache),
                        "palette": sample_palette(source, theme, cache),
                    }
                )
        except (ValueError, KeyError, TypeError, OSError, GLib.Error) as error:
            # Keep missing/broken optional icons from breaking the result list.
            fallback = (
                resolve("image://icon/text-x-generic", theme, cache)
                if source.startswith(FILE_PREVIEW_PREFIX)
                else source
            )
            emit({"source": source, "resolved": fallback, "error": str(error)})

    GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, read_request)
    emit({"ready": True})
    loop.run()


if __name__ == "__main__":
    main()
