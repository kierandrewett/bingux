// bingux-shell: Quickshell built into one Bingux binary.
//
//   bingux-shell MODE [--source DIR | --archive FILE] [quickshell options]
//   bingux-shell -p FILE.qml ...      (plain Quickshell, for tests and tools)
//   bingux-shell --shell-dir [--archive FILE]   (print the unpacked shell path)
//
// MODE selects which Bingux shell to start: desktop, lock, greeter, popouts,
// wallpaper, search, switcher, emoji, capture. The QML for every mode comes
// from the packed shell archive next to the binary's install prefix, unpacked
// into the user's cache on first use; --source runs it from a checkout
// instead. The binary also answers to its old names (bingux, bingux-lock,
// bingux-greeter, ...) through argv[0], so greetd, systemd units and user
// configurations written for the wrappers keep working.
//
// The environment the wrappers used to set is set here: the Bingux QML
// plugins on QML_IMPORT_PATH, the prefix on PATH and XDG_DATA_DIRS, and the
// helper binaries Bingux calls. Values already in the environment win.

#include "launch/main.hpp"

#include "archive.hpp"

#include <qcoreapplication.h>
#include <qdir.h>
#include <qfileinfo.h>
#include <qstring.h>
#include <qstringlist.h>
#include <qtenvironmentvariables.h>
#include <sys/resource.h>

#include <cstdio>
#include <cstring>
#include <vector>

namespace {

struct Mode {
    const char* name;
    const char* wrapper;
    const char* entry;
};

// Entry files are the fallback for --source trees; the archive's manifest is
// authoritative when the shell runs from the archive.
const Mode MODES[] = {
    {"desktop", "bingux", "shell.qml"},
    {"lock", "bingux-lock", "LockShell.qml"},
    {"greeter", "bingux-greeter", "GreeterShell.qml"},
    {"popouts", "bingux-popouts", "PopoutsShell.qml"},
    {"wallpaper", "bingux-wallpaper", "WallpaperShell.qml"},
    {"search", "bingux-search-ui", "SearchShell.qml"},
    {"switcher", "bingux-switcher-ui", "SwitcherShell.qml"},
    {"emoji", "bingux-emoji-ui", "EmojiShell.qml"},
    {"capture", "bingux-capture-ui", "CaptureShell.qml"},
};

const Mode* modeByName(const QString& name) {
    for (const auto& mode : MODES)
        if (name == QLatin1String(mode.name))
            return &mode;
    return nullptr;
}

const Mode* modeByWrapper(const QString& name) {
    for (const auto& mode : MODES)
        if (name == QLatin1String(mode.wrapper))
            return &mode;
    return nullptr;
}

void usage(FILE* stream) {
    fputs("usage: bingux-shell MODE [--source DIR | --archive FILE] [quickshell options]\n"
          "       bingux-shell -p FILE.qml [quickshell options]\n"
          "       bingux-shell --shell-dir [--archive FILE]\n"
          "       bingux-shell --help | --version\n"
          "\n"
          "modes:",
          stream);
    for (const auto& mode : MODES)
        fprintf(stream, " %s", mode.name);
    fputs("\n\n"
          "--source DIR    run the shell from a checkout (shell/bingux) instead of the archive\n"
          "--archive FILE  use this shell archive (default: BINGUX_SHELL_ARCHIVE or the installed "
          "one)\n"
          "--shell-dir     unpack the archive if needed and print the shell directory; tools such\n"
          "                as binguxctl address the running shell's IPC through this path\n"
          "\n"
          "The shell archive is unpacked into $XDG_CACHE_HOME/bingux/shell/<content id>/\n"
          "once per archive content. Quickshell options after the mode are passed through.\n",
          stream);
}

QString exePrefix() {
    auto exe = QFileInfo(QStringLiteral("/proc/self/exe")).symLinkTarget();
    if (exe.isEmpty())
        return {};
    return QFileInfo(QFileInfo(exe).dir().path()).dir().path();
}

QString libDir(const QString& prefix) {
    for (auto name :
         {QString::fromLatin1(BINGUX_LIBDIR), QStringLiteral("lib64"), QStringLiteral("lib")}) {
        auto candidate = prefix + QStringLiteral("/") + name + QStringLiteral("/bingux");
        if (QFileInfo(candidate).isDir())
            return candidate;
    }
    return prefix + QStringLiteral("/") + QString::fromLatin1(BINGUX_LIBDIR) +
           QStringLiteral("/bingux");
}

void setDefault(const char* name, const QString& value) {
    if (!qEnvironmentVariableIsSet(name) || qEnvironmentVariable(name).isEmpty())
        qputenv(name, value.toLocal8Bit());
}

// Install-tree paths are defaults only when they exist: a binary run from
// the build tree has no usable prefix, and the QML falls back on its own.
void setDefaultPath(const char* name, const QString& value) {
    if (QFileInfo::exists(value))
        setDefault(name, value);
}

void prepend(const char* name, const QString& value, const QString& fallback = {}) {
    auto current = qEnvironmentVariable(name);
    if (current.isEmpty())
        current = fallback;
    auto parts = current.split(QLatin1Char(':'), Qt::SkipEmptyParts);
    if (parts.contains(value))
        return;
    parts.prepend(value);
    qputenv(name, parts.join(QLatin1Char(':')).toLocal8Bit());
}

// What the generated wrappers exported before the single binary.
void applyEnvironment(const QString& prefix, const QString& shellDir) {
    auto lib = libDir(prefix);
    prepend("QML_IMPORT_PATH", lib + QStringLiteral("/qml"));
    prepend("XDG_DATA_DIRS", prefix + QStringLiteral("/share"),
            QStringLiteral("/usr/local/share:/usr/share"));
    prepend("PATH", prefix + QStringLiteral("/bin"));
    setDefaultPath("BINGUX_AUDIO_METER", prefix + QStringLiteral("/bin/bingux-audio-meter"));
    setDefaultPath("BINGUX_CONFIG_BIN", prefix + QStringLiteral("/bin/bingux-config"));
    setDefaultPath("BINGUX_HELPER_BIN", prefix + QStringLiteral("/bin/bingux-helper"));
    setDefaultPath("BINGUX_SETTINGS_BIN", prefix + QStringLiteral("/bin/bingux-settings"));
    setDefaultPath("BINGUX_HELPER_DIR", prefix + QStringLiteral("/libexec/bingux/shell"));
    setDefault("BINGUX_PYTHON", QStringLiteral("python3"));
    setDefault("BINGUX_QUICKSHELL", QFileInfo(QStringLiteral("/proc/self/exe")).symLinkTarget());
    if (!shellDir.isEmpty())
        qputenv("BINGUX_CONFIG_PATH", shellDir.toLocal8Bit());
}

QString defaultArchive(const QString& prefix) {
    auto configured = qEnvironmentVariable("BINGUX_SHELL_ARCHIVE");
    if (!configured.isEmpty())
        return configured;
    return libDir(prefix) + QStringLiteral("/") + QString::fromLatin1(BINGUX_SHELL_ARCHIVE_NAME);
}

} // namespace

int main(int argc, char** argv) {
    // Systemd restarts a crashed shell; a dump of a shell that holds the
    // screen is not worth the disk, and the lock screen must never write one.
    rlimit noCore{0, 0};
    setrlimit(RLIMIT_CORE, &noCore);

    auto prefix = exePrefix();
    auto invoked = QFileInfo(QString::fromLocal8Bit(argv[0])).fileName();
    const Mode* mode = modeByWrapper(invoked);
    int next = 1;
    if (mode == nullptr && argc > 1) {
        QString first = QString::fromLocal8Bit(argv[1]);
        if (first == QStringLiteral("--help") || first == QStringLiteral("-h") ||
            first == QStringLiteral("help")) {
            usage(stdout);
            return 0;
        }
        if (first == QStringLiteral("--version")) {
            printf("bingux-shell %s\n", BINGUX_SHELL_VERSION);
            return qs::launch::main(argc, argv);
        }
        if (first == QStringLiteral("--shell-dir")) {
            QString archive = argc > 3 && strcmp(argv[2], "--archive") == 0
                                  ? QString::fromLocal8Bit(argv[3])
                                  : defaultArchive(prefix);
            QString error;
            auto unpacked = bingux::unpackShellArchive(archive, &error);
            if (unpacked.isEmpty()) {
                fprintf(stderr, "bingux-shell: %s\n", qPrintable(error));
                return 1;
            }
            printf("%s/shell\n", qPrintable(unpacked));
            return 0;
        }
        if (first.startsWith(QLatin1Char('-'))) {
            // Plain Quickshell: tests and tools pass -p and their own options.
            applyEnvironment(prefix, {});
            return qs::launch::main(argc, argv);
        }
        mode = modeByName(first);
        if (mode == nullptr) {
            fprintf(stderr, "bingux-shell: unknown mode %s\n\n", argv[1]);
            usage(stderr);
            return 2;
        }
        next = 2;
    }
    if (mode == nullptr) {
        usage(stderr);
        return 2;
    }

    QString source = qEnvironmentVariable("BINGUX_SHELL_SOURCE");
    QString archive;
    std::vector<char*> passthrough;
    for (int i = next; i < argc; i++) {
        if (strcmp(argv[i], "--source") == 0 && i + 1 < argc)
            source = QString::fromLocal8Bit(argv[++i]);
        else if (strcmp(argv[i], "--archive") == 0 && i + 1 < argc)
            archive = QString::fromLocal8Bit(argv[++i]);
        else
            passthrough.push_back(argv[i]);
    }

    QString shellDir;
    QString entry;
    QString error;
    if (!source.isEmpty()) {
        shellDir = QFileInfo(source).absoluteFilePath();
        entry = shellDir + QStringLiteral("/") + QLatin1String(mode->entry);
        if (!QFileInfo::exists(entry)) {
            fprintf(stderr, "bingux-shell: %s is not a shell checkout (missing %s)\n",
                    qPrintable(shellDir), mode->entry);
            return 1;
        }
    } else {
        if (archive.isEmpty())
            archive = defaultArchive(prefix);
        auto unpacked = bingux::unpackShellArchive(archive, &error);
        if (unpacked.isEmpty()) {
            fprintf(stderr, "bingux-shell: %s\n", qPrintable(error));
            return 1;
        }
        entry = bingux::manifestEntry(unpacked, QLatin1String(mode->name), &error);
        if (entry.isEmpty()) {
            fprintf(stderr, "bingux-shell: %s\n", qPrintable(error));
            return 1;
        }
        shellDir = unpacked + QStringLiteral("/shell");
    }
    applyEnvironment(prefix, shellDir);

    auto entryBytes = entry.toLocal8Bit();
    std::vector<char*> args;
    args.push_back(argv[0]);
    args.push_back(const_cast<char*>("-p"));
    args.push_back(entryBytes.data());
    for (auto* arg : passthrough)
        args.push_back(arg);
    args.push_back(nullptr);
    return qs::launch::main(static_cast<int>(args.size()) - 1, args.data());
}
