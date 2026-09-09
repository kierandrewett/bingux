#!/usr/bin/env python3
"""Bingux capture worker: portal/PipeWire, optional compositor fast paths, H.264."""
import json
import ctypes
import os
from pathlib import Path
import shutil
import signal
import socket
import select
import subprocess
import sys
import tempfile
import time
import uuid
from concurrent.futures import ThreadPoolExecutor

import gi
gi.require_version("Gst", "1.0")
gi.require_version("GdkPixbuf", "2.0")
gi.require_version("GstVideo", "1.0")
from gi.repository import Gio, GLib, Gst, GdkPixbuf, GstVideo

PORTAL = "org.freedesktop.portal.Desktop"
PORTAL_PATH = "/org/freedesktop/portal/desktop"
MUTTER = "org.gnome.Mutter.ScreenCast"


class VideoCropMeta(ctypes.Structure):
    # Public GstVideoCropMeta ABI. PyGObject exposes get_meta() as Gst.Meta,
    # without the crop fields; read them while the buffer owns this metadata.
    _fields_ = [("flags", ctypes.c_uint), ("info", ctypes.c_void_p),
        ("x", ctypes.c_uint), ("y", ctypes.c_uint), ("width", ctypes.c_uint), ("height", ctypes.c_uint)]


def window_crop(buffer, width, height, fallback=None):
    meta = buffer.get_meta(GstVideo.video_crop_meta_api_get_type())
    if meta is None:
        if not fallback:
            raise ValueError("Window stream did not provide crop bounds")
        # Older PipeWire GStreamer plugins omit VideoCrop metadata. Mutter
        # draws the backing buffer at (0, 0); use its compositor pixel bounds.
        crop = VideoCropMeta(0, None, 0, 0, min(width, fallback["bufferWidth"]), min(height, fallback["bufferHeight"]))
    else:
        crop = VideoCropMeta.from_address(hash(meta))
    if not crop.width or not crop.height or crop.x + crop.width > width or crop.y + crop.height > height:
        raise ValueError("Window stream provided invalid crop bounds")
    return dict(left=crop.x, top=crop.y, right=width - crop.x - crop.width, bottom=height - crop.y - crop.height)


def play_shutter():
    """Use the GNOME sound event without delaying capture or overriding mute."""
    player = shutil.which("canberra-gtk-play")
    if not player:
        return
    command = [player, "--id", "screen-capture", "--description", "Screenshot taken"]
    source = Gio.SettingsSchemaSource.get_default()
    schema = source.lookup("org.gnome.desktop.sound", True) if source else None
    if schema:
        settings = Gio.Settings.new_full(schema, None, None)
        if not settings.get_boolean("event-sounds"):
            return
        command += ["--property", "canberra.xdg-theme.name=" + settings.get_string("theme-name")]
    try:
        subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, start_new_session=True)
    except OSError:
        pass  # Sound availability must not prevent saving a screenshot.


def capture_windows():
    path = os.environ.get("GNOBLIN_COMPOSITOR_SOCKET") or str(Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "gnoblin/compositor-v1.sock")
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(2)
        connection.connect(path)
        connection.sendall(b'{"op":"command","id":"capture-windows","command":"capture-windows"}\n')
        with connection.makefile("rb") as stream:
            for _ in range(8):
                line = stream.readline(1024 * 1024)
                if not line: break
                reply = json.loads(line)
                if reply.get("id") == "capture-windows":
                    if reply.get("event") == "error": raise ValueError(reply.get("message", "Window picker unavailable"))
                    return reply["result"]["windows"]
    raise ValueError("Window picker did not respond")


def settings(record):
    result = dict(record)
    for key, choices, default in (
        ("kind", ("screenshot", "recording"), "screenshot"),
        ("target", ("region", "window", "screen"), "region"),
        ("format", ("png", "jpeg"), "png"),
        ("quality", ("compact", "balanced", "high"), "balanced"),
        ("audio", ("none", "system", "microphone", "both"), "none"),
        ("backend", ("auto", "portal"), "auto"),
        ("encoder", ("auto", "cpu"), "auto"),
    ):
        result[key] = record.get(key, default)
        if result[key] not in choices:
            raise ValueError("Invalid " + key)
    for key, values, default in (("fps", (15, 30, 60), 30), ("maxHeight", (0, 720, 1080, 1440, 2160), 1080), ("delay", (0, 1, 2, 3, 5, 10), 0)):
        result[key] = int(record.get(key, default))
        if result[key] not in values:
            raise ValueError("Invalid " + key)
    for key in ("cursor", "copy"):
        result[key] = bool(record.get(key, key == "copy"))
    if result["target"] == "region":
        region = record.get("region", {})
        result["region"] = {key: int(region.get(key, 0)) for key in ("x", "y", "width", "height")}
        if not 2 <= result["region"]["width"] <= 16384 or not 2 <= result["region"]["height"] <= 16384:
            raise ValueError("Select a region at least 2 × 2 pixels")
    if result["target"] == "window" and record.get("windowId") is not None:
        identity = str(record["windowId"])
        if not identity.isdecimal() or not 0 < int(identity) < 2**64:
            raise ValueError("Invalid window ID")
        result["windowId"] = identity
    return result


def video_size(width, height, maximum):
    factor = min(1, maximum / height) if maximum else 1
    return max(2, int(width * factor) // 2 * 2), max(2, int(height * factor) // 2 * 2)


def bitrate(quality, width, height, fps):
    # Scale compression with the number of pixels/frames; no raw-video defaults.
    base = {"compact": 2500, "balanced": 6000, "high": 12000}[quality]
    return max(500, min(60000, round(base * width * height / (1920 * 1080) * fps / 30)))


def region_crop(region, metadata, width, height):
    position, size = metadata.get("position"), metadata.get("size")
    if not position or not size or min(size) <= 0:
        raise ValueError("Portal cannot map regions on this desktop; use full screen or a window")
    x, y = region["x"] - position[0], region["y"] - position[1]
    if min(x, y, size[0] - x - region["width"], size[1] - y - region["height"]) < 0:
        raise ValueError("Select the same monitor as your region in the desktop picker")
    left, top = round(x * width / size[0]), round(y * height / size[1])
    right = width - round((x + region["width"]) * width / size[0])
    bottom = height - round((y + region["height"]) * height / size[1])
    return dict(left=left, top=top, right=right, bottom=bottom)


class Capture:
    def __init__(self):
        Gst.init(None)
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.loop = GLib.MainLoop()
        self.preview_dir = tempfile.TemporaryDirectory(prefix="bingux-capture-", dir=os.environ.get("XDG_RUNTIME_DIR"))
        self.previews = {}
        self.preview_token = ""
        self.pending = bytearray()
        self.pipeline = None
        self.session = ""
        self.portal_session = False
        self.remote_fd = -1
        self.subscriptions = []
        self.request_path = ""
        self.job = None
        self.timer = 0
        self.stop_timer = 0
        self.final = None
        self.temporary = None
        self.stopping = False
        self.started = False
        self.encoder_checks = {}
        self.native = self.call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", "(s)", (MUTTER,))[0]
        try:
            self.portal_properties = self.call(PORTAL, PORTAL_PATH, "org.freedesktop.DBus.Properties", "GetAll", "(s)", ("org.freedesktop.portal.ScreenCast",))[0]
        except GLib.Error:
            self.portal_properties = {}
        try:
            capture_windows()
            self.window_picker = self.native
        except (OSError, ValueError, KeyError):
            self.window_picker = False
        self.encoders = [name for name in ("vah264enc", "nvh264enc", "x264enc", "openh264enc") if Gst.ElementFactory.find(name)]

    def emit(self, event, **fields):
        print(json.dumps(dict(event=event, **fields)), flush=True)

    def call(self, destination, path, interface, method, signature=None, arguments=()):
        return self.bus.call_sync(destination, path, interface, method, GLib.Variant(signature, arguments) if signature else None, None, Gio.DBusCallFlags.NONE, 5000, None).unpack()

    def run(self):
        GLib.io_add_watch(sys.stdin, GLib.IO_IN | GLib.IO_HUP, self.read)
        for sig in (signal.SIGINT, signal.SIGTERM):
            GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, sig, self.shutdown)
        self.emit("ready", native=self.native, portal=bool(self.portal_properties), window=self.window_picker or bool(self.portal_properties.get("AvailableSourceTypes", 0) & 2), windowPicker=self.window_picker, encoders=self.encoders,
                  audio=bool(Gst.ElementFactory.find("pulsesrc") and Gst.ElementFactory.find("avenc_aac")))
        try:
            self.loop.run()
        finally:
            self.cleanup()
            self.preview_dir.cleanup()

    def read(self, _source, _condition):
        chunk = os.read(sys.stdin.fileno(), 65536)
        if not chunk:
            self.shutdown()
            return False
        self.pending.extend(chunk)
        if len(self.pending) > 32768 and b"\n" not in self.pending:
            self.pending.clear()
            self.emit("error", message="Capture request too large")
            return True
        while b"\n" in self.pending:
            line, _, rest = self.pending.partition(b"\n")
            self.pending[:] = rest
            try:
                if len(line) > 32768:
                    raise ValueError("Capture request too large")
                record = json.loads(line)
                command = record.get("command")
                if command == "preview" and self.job is None:
                    self.preview(record)
                elif command == "capture":
                    self.capture(record)
                elif command == "stop":
                    self.stop()
                elif command == "cancel":
                    self.cancel()
            except (ValueError, TypeError, OSError, GLib.Error, subprocess.SubprocessError) as error:
                self.fail(str(error))
        return True

    def clear_preview(self):
        for screen in self.previews.values():
            for path in screen["paths"].values():
                path.unlink(missing_ok=True)
        self.previews = {}
        self.preview_token = ""

    def preview(self, record):
        self.clear_preview()
        windows = capture_windows() if self.window_picker else []
        token = uuid.uuid4().hex
        tasks = []
        for index, screen in enumerate(record.get("screens", [])[:16]):
            paths = {kind: Path(self.preview_dir.name) / f"{token}-{index}-{kind}.ppm" for kind in ("plain", "cursor")}
            self.previews[screen["name"]] = dict(paths=paths, geometry=screen)
            for kind, path in paths.items():
                command = ["grim", "-t", "ppm", "-o", screen["name"]]
                if kind == "cursor": command.append("-c")
                tasks.append(command + [str(path)])
        try:
            # Request both cursor variants together. PPM avoids PNG compression
            # on the opening path, and lives only in private runtime storage.
            with ThreadPoolExecutor(max_workers=4) as pool:
                results = list(pool.map(lambda command: subprocess.run(command, capture_output=True, timeout=4).returncode, tasks))
            if any(results): raise ValueError("This compositor cannot freeze the capture preview")
            self.preview_token = token
            self.emit("preview", request=record.get("request"), token=token, windows=windows,
                      images={name: {kind: path.as_uri() for kind, path in screen["paths"].items()} for name, screen in self.previews.items()})
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            self.clear_preview()
            self.emit("preview", request=record.get("request"), token="", images={}, windows=windows, warning=str(error))

    def frozen_screenshot(self):
        if not self.job.get("previewToken") or self.job["previewToken"] != self.preview_token or self.job["delay"] or self.job["target"] == "window":
            return False
        frame = self.previews.get(self.job.get("screen"))
        if not frame: return False
        image = GdkPixbuf.Pixbuf.new_from_file(str(frame["paths"]["cursor" if self.job["cursor"] else "plain"]))
        if self.job["target"] == "region":
            geometry = frame["geometry"]
            crop = region_crop(self.job["region"], dict(position=(geometry["x"], geometry["y"]), size=(geometry["width"], geometry["height"])), image.get_width(), image.get_height())
            image = image.new_subpixbuf(crop["left"], crop["top"], image.get_width() - crop["left"] - crop["right"], image.get_height() - crop["top"] - crop["bottom"])
        options = {"compression": "6"} if self.job["format"] == "png" else {"quality": str({"compact": 75, "balanced": 90, "high": 98}[self.job["quality"]])}
        image.savev(str(self.temporary), self.job["format"], list(options), list(options.values()))
        return True

    def capture(self, record):
        if self.job is not None:
            raise ValueError("A capture is already active")
        self.job = settings(record)
        self.stopping = False
        self.started = False
        self.emit("countdown", seconds=self.job["delay"])
        self.timer = GLib.timeout_add(max(180, self.job["delay"] * 1000), self.begin)

    def reserve_output(self):
        kind = self.job["kind"]
        directory = str(self.job.get("directory", "")).strip()
        if not directory:
            folder_type = GLib.UserDirectory.DIRECTORY_VIDEOS if kind == "recording" else GLib.UserDirectory.DIRECTORY_PICTURES
            directory = str(Path(GLib.get_user_special_dir(folder_type) or str(Path.home())) / ("Recordings" if kind == "recording" else "Screenshots"))
        folder = Path(directory).expanduser()
        if not folder.is_absolute():
            raise ValueError("Save folder must be an absolute path")
        folder.mkdir(parents=True, exist_ok=True)
        extension = "mp4" if kind == "recording" else self.job["format"]
        self.final = folder / (time.strftime("Recording %Y-%m-%d %H-%M-%S") if kind == "recording" else time.strftime("Screenshot %Y-%m-%d %H-%M-%S"))
        self.final = self.final.with_name(self.final.name + "-" + uuid.uuid4().hex[:6] + "." + extension)
        fd, path = tempfile.mkstemp(prefix=".bingux-capture-", suffix="." + extension, dir=folder)
        os.close(fd)
        self.temporary = Path(path)

    def begin(self):
        self.timer = 0
        try:
            self.reserve_output()
            self.emit("starting", target=self.job["target"])
            if self.job["kind"] == "screenshot" and self.frozen_screenshot():
                self.complete()
                return False
            self.clear_preview()
            if self.job["kind"] == "screenshot" and self.job["target"] != "window" and self.job["backend"] == "auto" and shutil.which("grim"):
                if self.grim():
                    self.complete()
                    return False
            if self.native and self.job["backend"] == "auto" and (self.job["target"] != "window" or self.job.get("windowId")):
                self.native_stream()
            else:
                self.portal_stream()
        except (ValueError, OSError, GLib.Error, subprocess.SubprocessError) as error:
            self.fail(str(error))
        return False

    def grim(self):
        command = ["grim", "-t", self.job["format"]]
        if self.job["cursor"]:
            command.append("-c")
        if self.job["format"] == "jpeg":
            command += ["-q", str({"compact": 75, "balanced": 90, "high": 98}[self.job["quality"]])]
        if self.job["target"] == "region":
            r = self.job["region"]
            command += ["-g", f'{r["x"]},{r["y"]} {r["width"]}x{r["height"]}']
        elif self.job.get("screen"):
            command += ["-o", self.job["screen"]]
        command.append(str(self.temporary))
        return subprocess.run(command, capture_output=True, timeout=8).returncode == 0

    def native_stream(self):
        self.portal_session = False
        self.session = self.call(MUTTER, "/org/gnome/Mutter/ScreenCast", MUTTER, "CreateSession", "(a{sv})", ({},))[0]
        properties = {"cursor-mode": GLib.Variant("u", int(self.job["cursor"])), "is-recording": GLib.Variant("b", self.job["kind"] == "recording")}
        if self.job["target"] == "window":
            self.refresh_window_bounds()
            properties["window-id"] = GLib.Variant("t", int(self.job["windowId"]))
            stream = self.call(MUTTER, self.session, MUTTER + ".Session", "RecordWindow", "(a{sv})", (properties,))[0]
        elif self.job["target"] == "region":
            r = self.job["region"]
            stream = self.call(MUTTER, self.session, MUTTER + ".Session", "RecordArea", "(iiiia{sv})", (r["x"], r["y"], r["width"], r["height"], properties))[0]
        else:
            stream = self.call(MUTTER, self.session, MUTTER + ".Session", "RecordMonitor", "(sa{sv})", (self.job.get("screen", ""), properties))[0]
        self.subscriptions.append(self.bus.signal_subscribe(MUTTER, MUTTER + ".Stream", "PipeWireStreamAdded", stream, None, Gio.DBusSignalFlags.NONE,
            lambda *args: self.start_pipeline(args[-1].unpack()[0])))
        self.call(MUTTER, self.session, MUTTER + ".Session", "Start")

    def portal_request(self, method, arguments, options, callback):
        token = "bingux" + uuid.uuid4().hex
        options = dict(options, handle_token=GLib.Variant("s", token))
        self.request_path = PORTAL_PATH + "/request/" + self.bus.get_unique_name()[1:].replace(".", "_") + "/" + token
        def response(*args):
            code, result = args[-1].unpack()
            self.request_path = ""
            if self.job is None:
                return
            if code:
                self.cancel()
                return
            try:
                callback(result)
            except (OSError, ValueError, GLib.Error) as error:
                self.fail(str(error))
        self.subscriptions.append(self.bus.signal_subscribe(PORTAL, "org.freedesktop.portal.Request", "Response", self.request_path, None, Gio.DBusSignalFlags.NONE, response))
        signature = {"CreateSession": "(a{sv})", "SelectSources": "(oa{sv})", "Start": "(osa{sv})"}[method]
        self.call(PORTAL, PORTAL_PATH, "org.freedesktop.portal.ScreenCast", method, signature, (*arguments, options))

    def portal_stream(self):
        source_type = 2 if self.job["target"] == "window" else 1
        if not self.portal_properties.get("AvailableSourceTypes", 0) & source_type:
            raise ValueError("This desktop portal does not support " + self.job["target"] + " capture")
        cursor = 2 if self.job["cursor"] else 1
        if not self.portal_properties.get("AvailableCursorModes", 1) & cursor:
            raise ValueError("This desktop cannot provide the requested cursor visibility")
        self.portal_session = True
        def created(result):
            self.session = result["session_handle"]
            self.portal_request("SelectSources", (self.session,), {"types": GLib.Variant("u", source_type), "multiple": GLib.Variant("b", False), "cursor_mode": GLib.Variant("u", cursor)}, selected)
        def selected(_result):
            self.portal_request("Start", (self.session, ""), {}, started)
        def started(result):
            node, metadata = result["streams"][0]
            if self.job["target"] == "region":
                self.job["portalRegion"] = metadata
            reply, descriptors = self.bus.call_with_unix_fd_list_sync(PORTAL, PORTAL_PATH, "org.freedesktop.portal.ScreenCast", "OpenPipeWireRemote", GLib.Variant("(oa{sv})", (self.session, {})), None, Gio.DBusCallFlags.NONE, 5000, None, None)
            self.remote_fd = descriptors.get(reply.unpack()[0])
            self.start_pipeline(node)
        self.portal_request("CreateSession", (), {"session_handle_token": GLib.Variant("s", "session" + uuid.uuid4().hex)}, created)

    def start_pipeline(self, node):
        if self.job is None:
            return
        try:
            recording = self.job["kind"] == "recording"
            source = f'pipewiresrc name=capture path={int(node)} do-timestamp=true'
            if self.remote_fd >= 0:
                source += f" fd={self.remote_fd}"
            if not recording:
                source += " num-buffers=1"
            else:
                source += " keepalive-time=100 resend-last=true"
            window_source = self.job["target"] == "window" and not self.portal_session
            crop = " ! videocrop name=windowcrop" if window_source else ""
            if "portalRegion" in self.job:
                metadata, r = self.job["portalRegion"], self.job["region"]
                region_crop(r, metadata, 100, 100)  # Validate before streaming.
                crop = " ! videocrop name=regioncrop"
            if recording:
                encoder = self.choose_encoder(self.job["encoder"])
                if not encoder:
                    raise ValueError("No working H.264 encoder; install GStreamer's x264 or OpenH264 plugin for CPU encoding")
                self.encoder = encoder
                conversion = "NV12" if encoder == "vah264enc" else "I420"
                video = source + crop + f' ! queue max-size-buffers=4 max-size-bytes=0 max-size-time=0 ! videorate ! videoconvert ! videoscale ! capsfilter name=sizing caps="video/x-raw,format={conversion},framerate={self.job["fps"]}/1" ! {encoder} name=encoder ! h264parse ! queue ! mux.'
                audio = self.audio_pipeline()
                pipeline = video + " " + audio + ' mp4mux name=mux faststart=true ! filesink name=output'
            else:
                encoder = "pngenc" if self.job["format"] == "png" else "jpegenc quality=" + str({"compact": 75, "balanced": 90, "high": 98}[self.job["quality"]])
                pipeline = source + crop + " ! videoconvert ! " + encoder + " ! filesink name=output"
            self.pipeline = Gst.parse_launch(pipeline)
            self.pipeline.get_by_name("output").set_property("location", str(self.temporary))
            if window_source:
                self.pipeline.get_by_name("capture").get_static_pad("src").add_probe(Gst.PadProbeType.BUFFER, self.window_crop_probe)
            if recording:
                self.pipeline.get_by_name("windowcrop" if window_source else "capture").get_static_pad("src").add_probe(Gst.PadProbeType.EVENT_DOWNSTREAM, self.size_probe)
            bus = self.pipeline.get_bus()
            bus.add_signal_watch()
            bus.connect("message", self.message)
            if self.pipeline.set_state(Gst.State.PLAYING) == Gst.StateChangeReturn.FAILURE:
                raise ValueError("Could not start the video encoder")
        except (GLib.Error, ValueError, TypeError, OSError, subprocess.SubprocessError) as error:
            self.fail(str(error))

    def refresh_window_bounds(self):
        selected = next((window for window in capture_windows() if window["id"] == self.job["windowId"]), None)
        if selected is None:
            raise ValueError("The selected window is no longer available")
        self.job["windowBounds"] = selected

    def window_crop_probe(self, pad, info):
        try:
            caps = pad.get_current_caps().get_structure(0)
            bounds = window_crop(info.get_buffer(), caps.get_value("width"), caps.get_value("height"), self.job.get("windowBounds"))
            element = self.pipeline.get_by_name("windowcrop")
            for key, value in bounds.items():
                if element.get_property(key) != value:
                    element.set_property(key, value)
        except (ValueError, TypeError) as error:
            GLib.idle_add(self.fail, str(error))
            return Gst.PadProbeReturn.DROP
        return Gst.PadProbeReturn.OK

    def choose_encoder(self, mode):
        return next((name for name in self.encoders
                     if (mode == "auto" or name in ("x264enc", "openh264enc"))
                     and self.encoder_works(name)), None)

    def size_probe(self, _pad, info):
        event = info.get_event()
        if event.type == Gst.EventType.CAPS and self.job and self.pipeline:
            structure = event.parse_caps().get_structure(0)
            width, height = structure.get_value("width"), structure.get_value("height")
            if "portalRegion" in self.job:
                crop = region_crop(self.job["region"], self.job["portalRegion"], width, height)
                element = self.pipeline.get_by_name("regioncrop")
                for key, value in crop.items():
                    element.set_property(key, value)
                width -= crop["left"] + crop["right"]
                height -= crop["top"] + crop["bottom"]
            if self.job["kind"] != "recording":
                return Gst.PadProbeReturn.OK
            width, height = video_size(width, height, self.job["maxHeight"])
            color = "NV12" if self.encoder == "vah264enc" else "I420"
            self.pipeline.get_by_name("sizing").set_property("caps", Gst.Caps.from_string(f"video/x-raw,format={color},width={width},height={height},framerate={self.job['fps']}/1"))
            encoder = self.pipeline.get_by_name("encoder")
            kbps = bitrate(self.job["quality"], width, height, self.job["fps"])
            encoder.set_property("bitrate", kbps * 1000 if self.encoder == "openh264enc" else kbps)
            if self.encoder == "vah264enc":
                encoder.set_property("rate-control", 4)  # VBR
            elif self.encoder == "x264enc":
                encoder.set_property("speed-preset", 3)  # veryfast
            elif self.encoder == "openh264enc":
                encoder.set_property("complexity", 0)
                encoder.set_property("rate-control", 1)  # bitrate
        return Gst.PadProbeReturn.OK

    def encoder_works(self, name):
        if name in self.encoder_checks:
            return self.encoder_checks[name]
        pipeline = None
        try:
            # A registered plugin is not proof of a usable GPU/driver. Probe a
            # few synthetic frames once, before touching the user's recording.
            color = "NV12" if name == "vah264enc" else "I420"
            pipeline = Gst.parse_launch(f"videotestsrc num-buffers=3 ! video/x-raw,width=640,height=360,framerate=15/1 ! videoconvert ! video/x-raw,format={color} ! {name} ! fakesink")
            pipeline.set_state(Gst.State.PLAYING)
            message = pipeline.get_bus().timed_pop_filtered(2 * Gst.SECOND, Gst.MessageType.ERROR | Gst.MessageType.EOS)
            works = message is not None and message.type == Gst.MessageType.EOS
        except GLib.Error:
            works = False
        finally:
            if pipeline:
                pipeline.set_state(Gst.State.NULL)
        self.encoder_checks[name] = works
        return works

    def audio_pipeline(self):
        mode = self.job["audio"]
        if mode == "none":
            return ""
        if not Gst.ElementFactory.find("avenc_aac") or not Gst.ElementFactory.find("pulsesrc"):
            raise ValueError("Audio recording needs pulsesrc and an AAC encoder")
        sources = []
        if mode in ("system", "both"):
            sink = subprocess.check_output(["pactl", "get-default-sink"], text=True, timeout=3).strip()
            sources.append("pulsesrc device=" + json.dumps(sink + ".monitor"))
        if mode in ("microphone", "both"):
            sources.append("pulsesrc")
        branches = " ".join(source + " ! queue ! audioconvert ! audioresample ! mix. " for source in sources)
        return branches + " audiomixer name=mix ! audioconvert ! avenc_aac bitrate=128000 ! aacparse ! queue ! mux. "

    def message(self, _bus, message):
        if message.type == Gst.MessageType.ERROR:
            error, debug = message.parse_error()
            self.fail(error.message + (" (try Software encoding)" if self.job and self.job["encoder"] == "auto" else ""))
        elif message.type == Gst.MessageType.EOS:
            self.complete()
        elif message.type == Gst.MessageType.STATE_CHANGED and message.src == self.pipeline:
            _, state, _ = message.parse_state_changed()
            if state == Gst.State.PLAYING and not self.started:
                self.started = True
                if self.job["kind"] == "recording":
                    self.emit("recording", path=str(self.final), encoder=self.encoder, started=time.time())

    def stop(self):
        if self.pipeline and self.job and self.job["kind"] == "recording":
            if self.stopping:
                return
            self.stopping = True
            self.emit("finalizing")
            self.pipeline.send_event(Gst.Event.new_eos())
            self.stop_timer = GLib.timeout_add_seconds(15, lambda: self.fail("Finalization timed out; partial output was preserved"))
        else:
            self.cancel()

    def complete(self):
        if self.job is None:
            return
        job, output, temporary = self.job, self.final, self.temporary
        self.cleanup()
        if not temporary or not temporary.exists() or temporary.stat().st_size == 0:
            self.emit("error", message="Capture produced no output")
            return
        os.replace(temporary, output)
        if job["kind"] == "screenshot":
            play_shutter()
        copied = False
        if job["kind"] == "screenshot" and job["copy"] and shutil.which("wl-copy"):
            try:
                with output.open("rb") as image:
                    copied = subprocess.run(["wl-copy", "--type", "image/" + job["format"]], stdin=image, timeout=4).returncode == 0
            except (OSError, subprocess.SubprocessError):
                pass  # Clipboard failure must not hide a successfully saved image.
        notified = False
        try:
            notifier = subprocess.Popen([sys.executable, str(Path(__file__).with_name("capture-notify.py")),
                str(output), job["kind"]], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, start_new_session=True)
            # Actions remain available after the capture worker or shell reloads.
            if select.select([notifier.stdout], [], [], 1)[0]:
                notified = notifier.stdout.readline().strip() == b"notified"
            notifier.stdout.close()
        except OSError:
            pass
        self.emit("saved", path=str(output), bytes=output.stat().st_size, copied=copied, kind=job["kind"], notified=notified)
        self.clear_preview()

    def cleanup(self):
        for timer in (self.timer, self.stop_timer):
            if timer:
                GLib.source_remove(timer)
        self.timer = self.stop_timer = 0
        if self.pipeline:
            self.pipeline.set_state(Gst.State.NULL)
            self.pipeline = None
        if self.session:
            try:
                self.call(PORTAL if self.portal_session else MUTTER, self.session, "org.freedesktop.portal.Session" if self.portal_session else MUTTER + ".Session", "Close" if self.portal_session else "Stop")
            except GLib.Error:
                pass
            self.session = ""
        for subscription in self.subscriptions:
            self.bus.signal_unsubscribe(subscription)
        self.subscriptions.clear()
        if self.remote_fd >= 0:
            os.close(self.remote_fd)
            self.remote_fd = -1
        self.job = None

    def cancel(self):
        if self.pipeline and self.job and self.job["kind"] == "recording":
            self.stop()
            return
        if self.request_path:
            try:
                self.call(PORTAL, self.request_path, "org.freedesktop.portal.Request", "Close")
            except GLib.Error:
                pass
            self.request_path = ""
        self.cleanup()
        if self.temporary and self.temporary.exists() and self.temporary.stat().st_size == 0:
            self.temporary.unlink()
        self.emit("cancelled")
        self.clear_preview()

    def fail(self, message):
        partial = str(self.temporary) if self.temporary and self.temporary.exists() and self.temporary.stat().st_size else ""
        self.cleanup()
        if not partial and self.temporary:
            self.temporary.unlink(missing_ok=True)
        self.emit("error", message=message, partial=partial)
        return False

    def shutdown(self):
        if self.pipeline and self.job and self.job["kind"] == "recording":
            self.stop()
            GLib.timeout_add_seconds(16, self.loop.quit)
        else:
            self.loop.quit()
        return False


if __name__ == "__main__":
    Capture().run()
