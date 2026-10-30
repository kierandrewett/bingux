#include "archive.hpp"

#include <archive.h>
#include <archive_entry.h>
#include <qcryptographichash.h>
#include <qdir.h>
#include <qfile.h>
#include <qfileinfo.h>
#include <qjsondocument.h>
#include <qjsonobject.h>
#include <qstandardpaths.h>
#include <unistd.h>

namespace bingux {

namespace {

// The cache key is the archive content, so a new package lands in a new
// directory and a shell that is still running keeps its old files.
QString contentId(const QString& path, QString* error) {
    QFile file(path);
    if (!file.open(QFile::ReadOnly)) {
        *error = QStringLiteral("cannot read %1: %2").arg(path, file.errorString());
        return {};
    }
    QCryptographicHash hash(QCryptographicHash::Sha256);
    if (!hash.addData(&file)) {
        *error = QStringLiteral("cannot hash %1").arg(path);
        return {};
    }
    return QString::fromLatin1(hash.result().toHex().left(16));
}

// $XDG_CACHE_HOME/bingux/shell, or a per-user directory under /tmp when the
// cache is not writable (the greeter account, a read-only home).
QString cacheRoot() {
    auto cache = QStandardPaths::writableLocation(QStandardPaths::GenericCacheLocation);
    if (!cache.isEmpty()) {
        QDir dir(cache + QStringLiteral("/bingux/shell"));
        if (dir.mkpath(QStringLiteral(".")) && QFileInfo(dir.path()).isWritable())
            return dir.path();
    }
    QDir fallback(QStringLiteral("/tmp/bingux-shell-%1").arg(getuid()));
    fallback.mkpath(QStringLiteral("."));
    return fallback.path();
}

bool extractTo(const QString& archivePath, const QString& destination, QString* error) {
    auto* reader = archive_read_new();
    archive_read_support_filter_xz(reader);
    archive_read_support_filter_zstd(reader);
    archive_read_support_format_tar(reader);
    auto* writer = archive_write_disk_new();
    // Members are checked below and then prefixed with the absolute staging
    // directory, so libarchive's own absolute-path refusal cannot apply.
    archive_write_disk_set_options(writer, ARCHIVE_EXTRACT_PERM | ARCHIVE_EXTRACT_TIME |
                                               ARCHIVE_EXTRACT_SECURE_NODOTDOT |
                                               ARCHIVE_EXTRACT_SECURE_SYMLINKS);
    auto fail = [&](const char* what) {
        *error = QStringLiteral("%1: %2").arg(QString::fromUtf8(what),
                                              QString::fromUtf8(archive_error_string(reader)));
        archive_write_free(writer);
        archive_read_free(reader);
        return false;
    };
    auto archiveBytes = archivePath.toLocal8Bit();
    if (archive_read_open_filename(reader, archiveBytes.constData(), 1 << 16) != ARCHIVE_OK)
        return fail("cannot open shell archive");
    auto destinationBytes = destination.toLocal8Bit();
    archive_entry* entry = nullptr;
    while (true) {
        auto status = archive_read_next_header(reader, &entry);
        if (status == ARCHIVE_EOF)
            break;
        if (status != ARCHIVE_OK)
            return fail("bad shell archive entry");
        auto member = QByteArray(archive_entry_pathname(entry));
        if (member.isEmpty() || member.startsWith('/') || member == ".." ||
            member.startsWith("../") || member.contains("/../") || member.endsWith("/..")) {
            *error = QStringLiteral("%1 has an unsafe member path: %2")
                         .arg(archivePath, QString::fromUtf8(member));
            archive_write_free(writer);
            archive_read_free(reader);
            return false;
        }
        auto path = QByteArray(destinationBytes) + '/' + member;
        archive_entry_set_pathname(entry, path.constData());
        if (archive_write_header(writer, entry) != ARCHIVE_OK) {
            *error =
                QStringLiteral("cannot write %1: %2")
                    .arg(QString::fromUtf8(path), QString::fromUtf8(archive_error_string(writer)));
            archive_write_free(writer);
            archive_read_free(reader);
            return false;
        }
        const void* block = nullptr;
        size_t size = 0;
        la_int64_t offset = 0;
        while (true) {
            status = archive_read_data_block(reader, &block, &size, &offset);
            if (status == ARCHIVE_EOF)
                break;
            if (status != ARCHIVE_OK)
                return fail("cannot read shell archive data");
            if (archive_write_data_block(writer, block, size, offset) != ARCHIVE_OK) {
                *error = QStringLiteral("cannot write %1: %2")
                             .arg(QString::fromUtf8(path),
                                  QString::fromUtf8(archive_error_string(writer)));
                archive_write_free(writer);
                archive_read_free(reader);
                return false;
            }
        }
        archive_write_finish_entry(writer);
    }
    archive_write_free(writer);
    archive_read_free(reader);
    return true;
}

} // namespace

// A cached tree counts only when its manifest parses. A power cut can leave the
// final name with an empty manifest.json (the rename reached the disk, the data
// did not); such a tree is removed and unpacked again.
static bool completeUnpack(const QString& dir) {
    QFile file(dir + QStringLiteral("/manifest.json"));
    if (!file.open(QFile::ReadOnly))
        return false;
    auto document = QJsonDocument::fromJson(file.readAll());
    return document.isObject() &&
           document.object().value(QStringLiteral("format")).toString() ==
               QStringLiteral("bingux-shell");
}

QString unpackShellArchive(const QString& archivePath, QString* error) {
    auto id = contentId(archivePath, error);
    if (id.isEmpty())
        return {};
    auto root = cacheRoot();
    auto target = root + QStringLiteral("/") + id;
    if (completeUnpack(target))
        return target;
    if (QFileInfo::exists(target))
        QDir(target).removeRecursively();

    // Unpack beside the target and rename, so a crash or two shells starting
    // at once never leave a half-written tree under the final name.
    auto staging = QStringLiteral("%1/.%2.%3").arg(root, id).arg(getpid());
    QDir(staging).removeRecursively();
    if (!QDir().mkpath(staging)) {
        *error = QStringLiteral("cannot create %1").arg(staging);
        return {};
    }
    if (!extractTo(archivePath, staging, error)) {
        QDir(staging).removeRecursively();
        return {};
    }
    if (!QFileInfo::exists(staging + QStringLiteral("/manifest.json"))) {
        QDir(staging).removeRecursively();
        *error = QStringLiteral("%1 has no manifest.json").arg(archivePath);
        return {};
    }
    // The data must reach the disk before the rename does, or a power cut can
    // leave the final name with an empty tree.
    ::sync();
    if (!QDir().rename(staging, target)) {
        QDir(staging).removeRecursively();
        if (!QFileInfo::exists(target + QStringLiteral("/manifest.json"))) {
            *error = QStringLiteral("cannot move the unpacked shell into %1").arg(target);
            return {};
        }
    }
    return target;
}

QString manifestEntry(const QString& unpackedDir, const QString& mode, QString* error) {
    QFile file(unpackedDir + QStringLiteral("/manifest.json"));
    if (!file.open(QFile::ReadOnly)) {
        *error = QStringLiteral("cannot read %1").arg(file.fileName());
        return {};
    }
    QJsonParseError parseError{};
    auto document = QJsonDocument::fromJson(file.readAll(), &parseError);
    if (!document.isObject()) {
        *error = QStringLiteral("%1: %2").arg(file.fileName(), parseError.errorString());
        return {};
    }
    auto object = document.object();
    if (object.value(QStringLiteral("format")).toString() != QStringLiteral("bingux-shell")) {
        *error = QStringLiteral("%1 is not a bingux-shell manifest").arg(file.fileName());
        return {};
    }
    auto entry = object.value(QStringLiteral("entries")).toObject().value(mode).toString();
    if (entry.isEmpty()) {
        *error = QStringLiteral("the shell archive has no entry for mode %1").arg(mode);
        return {};
    }
    return unpackedDir + QStringLiteral("/shell/") + entry;
}

} // namespace bingux
