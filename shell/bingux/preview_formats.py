"""Bounded, read-only adapters for office documents, databases and media."""
import hashlib
import base64
import json
import os
from pathlib import Path
import shutil
import signal
import sqlite3
import subprocess
import tempfile
import time
from preview_limits import check_file

OFFICE_SUFFIXES = {'.doc', '.docx', '.odt', '.rtf', '.ppt', '.pptx', '.odp', '.xls', '.xlsx', '.ods'}
DATABASE_SUFFIXES = {'.db', '.sqlite', '.sqlite3', '.db3'}
MEDIA_SUFFIXES = {'.mp4', '.mkv', '.webm', '.mov', '.avi', '.m4v', '.mpeg', '.mpg', '.ogv', '.mp3', '.flac', '.ogg', '.wav', '.m4a', '.opus'}


def run_tool(command, timeout, capture_output=False, binary=False):
    """Cancel the whole converter/probe when its preview request is replaced."""
    process = subprocess.Popen(command, stdout=subprocess.PIPE if capture_output else subprocess.DEVNULL,
                               stderr=subprocess.DEVNULL, text=not binary, start_new_session=True)
    def stop(signum, _frame):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        raise SystemExit(128 + signum)
    previous = signal.signal(signal.SIGTERM, stop)
    try:
        output, _ = process.communicate(timeout=timeout)
        if process.returncode:
            raise ValueError('This file could not be read by its preview renderer.')
        return output
    except subprocess.TimeoutExpired:
        raise ValueError('This file took too long to prepare for preview.') from None
    finally:
        signal.signal(signal.SIGTERM, previous)
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()


def office_pdf(path):
    check_file(path)
    executable = shutil.which('libreoffice') or shutil.which('soffice')
    if not executable:
        raise ValueError('LibreOffice is needed to preview this document.')
    stat = path.stat()
    identity = hashlib.sha256(f'{path}:{stat.st_ino}:{stat.st_mtime_ns}:{stat.st_ctime_ns}:{stat.st_size}'.encode()).hexdigest()
    cache = Path(os.environ.get('XDG_CACHE_HOME', Path.home() / '.cache')) / 'bingux/document-previews'
    cache.mkdir(parents=True, exist_ok=True, mode=0o700)
    result = cache / (identity + '.pdf')
    if result.is_file():
        result.touch()
        return result
    with tempfile.TemporaryDirectory(prefix='convert-', dir=cache) as temporary:
        work = Path(temporary)
        profile = work / 'profile'
        (profile / 'user').mkdir(parents=True)
        (profile / 'user/registrymodifications.xcu').write_text('''<?xml version="1.0"?><oor:items xmlns:oor="http://openoffice.org/2001/registry"><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item><item oor:path="/org.openoffice.Office.Common/Load"><prop oor:name="UpdateLinks" oor:op="fuse"><value>0</value></prop></item></oor:items>''')
        # An isolated profile prevents joining or changing a running office session.
        run_tool([executable, '-env:UserInstallation=' + profile.as_uri(), '--headless', '--nologo', '--nodefault', '--norestore', '--convert-to', 'pdf', '--outdir', str(work), str(path)], timeout=22)
        converted = work / (path.stem + '.pdf')
        if not converted.is_file():
            raise ValueError('This document could not be converted for preview.')
        converted.replace(result)
    # Keep recently used conversions, bounded by count and total disk use.
    used = 0
    for index, item in enumerate(sorted(cache.glob('*.pdf'), key=lambda item: item.stat().st_mtime, reverse=True)):
        used += item.stat().st_size
        if index >= 16 or (used > 512 * 1024 * 1024 and item != result):
            item.unlink(missing_ok=True)
    return result


def database(path, table=None, offset=0):
    check_file(path)
    connection = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True, timeout=1)
    try:
        connection.execute('PRAGMA query_only=ON')
        deadline = time.monotonic() + 2
        connection.set_progress_handler(lambda: time.monotonic() > deadline, 1000)
        tables = [row[0] for row in connection.execute("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name LIMIT 100")]
        table = table if table in tables else (tables[0] if tables else '')
        if not table:
            return {'kind': 'database', 'tables': [], 'table': '', 'columns': [], 'rows': [], 'offset': 0, 'hasMore': False}
        quoted = '"' + table.replace('"', '""') + '"'
        columns = [row[1] for row in connection.execute('PRAGMA table_info(' + quoted + ')')][:50]
        selection = ','.join('"' + name.replace('"', '""') + '"' for name in columns)
        offset = max(0, min(1_000_000, offset))
        rows = connection.execute(f'SELECT {selection} FROM {quoted} LIMIT 101 OFFSET ?', (offset,)).fetchall()
        def cell(value):
            if isinstance(value, bytes):
                return f'[{len(value)} bytes]'
            return 'NULL' if value is None else str(value)[:512]
        return {'kind': 'database', 'tables': tables, 'table': table, 'columns': columns,
                'rows': [[cell(value) for value in row] for row in rows[:100]], 'offset': offset, 'hasMore': len(rows) > 100}
    finally:
        connection.close()


def media(path):
    check_file(path)
    executable = shutil.which('ffprobe')
    if not executable:
        raise ValueError('FFmpeg is needed to inspect this media file.')
    result = run_tool([executable, '-v', 'error', '-show_entries', 'format=duration:stream=codec_type,codec_name,width,height', '-of', 'json', str(path)], capture_output=True, timeout=5)
    data = json.loads(result)
    video = next((stream for stream in data.get('streams', []) if stream.get('codec_type') == 'video'), None)
    return {'kind': 'video' if video else 'audio', 'duration': float(data.get('format', {}).get('duration', 0)),
            'width': video.get('width', 0) if video else 0, 'height': video.get('height', 0) if video else 0,
            'codec': video.get('codec_name', '') if video else '', 'source': path.as_uri()}


def video_poster(path):
    check_file(path)
    executable = shutil.which('ffmpeg')
    if not executable:
        return ''
    pixels = run_tool([executable, '-v', 'error', '-i', str(path), '-frames:v', '1', '-vf',
                       'scale=1024:1024:force_original_aspect_ratio=decrease', '-f', 'image2pipe', '-c:v', 'png', 'pipe:1'],
                      timeout=5, capture_output=True, binary=True)
    return 'data:image/png;base64,' + base64.b64encode(pixels).decode('ascii')


def formatted_text(text, kind, path):
    """Use document markup without executing scripts or fetching remote assets."""
    from html import escape
    from html.parser import HTMLParser
    from urllib.parse import unquote, urlparse
    if kind == 'markdown':
        import markdown
        text = markdown.markdown(text, extensions=['extra', 'sane_lists'])

    class ReaderHTML(HTMLParser):
        def __init__(self):
            super().__init__(convert_charrefs=True)
            self.output = []
            self.hidden = 0

        def handle_starttag(self, tag, attrs):
            if tag in {'script', 'style', 'head', 'iframe', 'object'}:
                self.hidden += 1
            if self.hidden:
                return
            if tag not in {'p', 'br', 'hr', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'b', 'strong', 'i', 'em', 'u', 's', 'pre', 'code', 'blockquote', 'ul', 'ol', 'li', 'table', 'thead', 'tbody', 'tr', 'td', 'th', 'div', 'span', 'img', 'a', 'sup', 'sub'}:
                return
            attributes = []
            for key, value in attrs:
                if key in {'colspan', 'rowspan'} and value and value.isdigit():
                    attributes.append((key, value))
                elif key == 'src' and tag == 'img' and value:
                    parsed = urlparse(value)
                    if parsed.scheme == '' and not parsed.netloc:
                        local = path.parent / unquote(parsed.path)
                        if local.resolve().is_relative_to(path.parent.resolve()) and local.is_file() and local.stat().st_size < 16 * 1024 * 1024:
                            attributes.append(('src', local.resolve().as_uri()))
                    elif value.startswith('data:image/'):
                        attributes.append(('src', value))
                elif key == 'alt' and value:
                    attributes.append((key, value))
            if tag == 'img' and not any(key == 'src' for key, _ in attributes):
                return
            self.output.append('<' + tag + ''.join(' ' + key + '="' + escape(value, quote=True) + '"' for key, value in attributes) + '>')

        def handle_endtag(self, tag):
            if tag in {'script', 'style', 'head', 'iframe', 'object'}:
                self.hidden = max(0, self.hidden - 1)
                return
            if not self.hidden:
                self.output.append('</' + tag + '>')

        def handle_data(self, data):
            if not self.hidden:
                self.output.append(escape(data))

    reader = ReaderHTML()
    reader.feed(text)
    return ''.join(reader.output)
