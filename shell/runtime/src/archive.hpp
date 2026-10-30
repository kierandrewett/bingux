#pragma once

#include <qstring.h>

namespace bingux {

// The packed shell archive (bingux-shell.sxa) is a tar.xz with manifest.json
// at its root and the QML tree under shell/. It is unpacked once per
// archive content into the user's cache and reused from there, so Qt's
// compiled-QML cache keeps working on ordinary local files.

// Unpack `archivePath` if this content is not cached yet and return the
// directory that holds manifest.json. Empty on failure, with `error` set.
QString unpackShellArchive(const QString& archivePath, QString* error);

// The entry file for `mode` from the manifest in `unpackedDir`, as an
// absolute path, or empty when the manifest does not list it.
QString manifestEntry(const QString& unpackedDir, const QString& mode, QString* error);

} // namespace bingux
