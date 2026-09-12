"""Small, bounded previews. Archives are listed, never extracted or executed."""

import csv
import io
import json
import tarfile
import tomllib
import zipfile
from email import policy
from email.parser import BytesParser
from html import escape

EXTRA_SUFFIXES = {".zip", ".tar", ".tgz", ".gz", ".bz2", ".xz", ".csv", ".tsv", ".eml", ".epub"}
LIMIT = 256 * 1024


def frontmatter(text):
    lines = text.splitlines(keepends=True)
    if not lines or lines[0].strip("\ufeff\r\n") not in ("---", "+++"):
        return text, {}, ""
    marker = lines[0].strip("\ufeff\r\n")
    end = next((i for i in range(1, min(len(lines), 256)) if lines[i].strip() == marker), None)
    if end is None:
        return text, {}, ""
    source = "".join(lines[1:end])
    if len(source.encode()) > 16384:
        return text, {}, "Frontmatter is too large to display."
    try:
        if marker == "+++":
            data = tomllib.loads(source)
        else:
            import yaml

            # Do not expand aliases or construct application objects.
            tokens = list(yaml.scan(source))
            if len(tokens) > 2048 or any(isinstance(t, yaml.tokens.AliasToken) for t in tokens):
                raise ValueError("Frontmatter aliases are not supported")
            data = yaml.safe_load(source)
        if data is None:
            data = {}
        if not isinstance(data, dict):
            raise ValueError("Expected a frontmatter mapping")
        facts = {}
        for key, value in list(data.items())[:32]:
            rendered = (
                ", ".join(str(item) for item in value)
                if isinstance(value, list) and all(not isinstance(item, (dict, list)) for item in value)
                else json.dumps(value, ensure_ascii=False, default=str)
                if isinstance(value, (dict, list))
                else str(value)
            )
            facts[str(key)[:80]] = rendered[:512]
        return "".join(lines[end + 1 :]), facts, ""
    except (ValueError, TypeError, RecursionError, ImportError):
        return text, {}, "Frontmatter could not be parsed."
    except Exception:
        return text, {}, "Frontmatter could not be parsed."


def table(headers, rows):
    def cell(value, tag="td"):
        return f"<{tag}>{escape(str(value)[:512])}</{tag}>"

    return (
        '<table cellspacing="8"><tr>'
        + "".join(cell(h, "th") for h in headers)
        + "</tr>"
        + "".join("<tr>" + "".join(cell(v) for v in row) + "</tr>" for row in rows)
        + "</table>"
    )


def preview(path, sanitise):
    suffix = path.suffix.lower()
    if suffix in (".csv", ".tsv"):
        with path.open("rb") as stream:
            data = stream.read(LIMIT + 1)
        reader = csv.reader(
            io.StringIO(data[:LIMIT].decode("utf-8-sig", errors="replace")), delimiter="\t" if suffix == ".tsv" else ","
        )
        rows = []
        for row in reader:
            rows.append(row[:30])
            if len(rows) >= 101:
                break
        return {
            "kind": "text",
            "format": "table",
            "text": "",
            "renderedText": table(rows[0] if rows else [], rows[1:]),
            "truncated": len(data) > LIMIT or len(rows) >= 101,
        }
    if suffix == ".eml":
        with path.open("rb") as stream:
            data = stream.read(LIMIT + 1)
        mail = BytesParser(policy=policy.default).parsebytes(data[:LIMIT])
        facts = {key: str(mail[key])[:512] for key in ("Subject", "From", "To", "Date") if mail[key]}
        body = mail.get_body(preferencelist=("plain", "html"))
        content = body.get_content() if body else ""
        rendered = (
            sanitise(content, "html", path)
            if body and body.get_content_type() == "text/html"
            else "<pre>" + escape(content) + "</pre>"
        )
        return {
            "kind": "text",
            "format": "email",
            "text": content,
            "renderedText": rendered,
            "frontmatter": facts,
            "truncated": len(data) > LIMIT,
        }
    rows = []
    if suffix in (".zip", ".epub"):
        with zipfile.ZipFile(path) as archive:
            entries = archive.infolist()
            for item in entries[:200]:
                rows.append([item.filename, f"{item.file_size:,} B"])
            count = len(entries)
    else:
        # Streaming iteration avoids decompressing the entire archive into memory.
        with tarfile.open(path, mode="r|*") as archive:
            for item in archive:
                rows.append([item.name, f"{item.size:,} B"])
                if len(rows) >= 200:
                    break
        count = len(rows)
    return {
        "kind": "text",
        "format": "archive",
        "text": "",
        "renderedText": table(["Name", "Size"], rows),
        "truncated": count >= 200,
        "frontmatter": {"Contents": f"{count}{'+' if count >= 200 else ''} entries"},
    }
