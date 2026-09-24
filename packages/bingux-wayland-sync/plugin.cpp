#include <QGuiApplication>
#include <QQmlExtensionPlugin>
#include <qqml.h>
#include <wayland-client.h>

#include <cerrno>
#include <qpa/qplatformnativeinterface.h>

namespace {

class WaylandRoundtrip : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool pending READ pending NOTIFY pendingChanged)

  public:
    using QObject::QObject;

    ~WaylandRoundtrip() override {
        clearCallback();
    }

    bool pending() const {
        return mCallback != nullptr;
    }

    // Queues wl_display.sync on Qt's existing Wayland connection. Requests on
    // one connection are ordered, so its callback proves the compositor has
    // processed requests Qt queued before this call, including unlock_and_destroy.
    Q_INVOKABLE bool synchronize() {
        if (mCallback)
            return false;

        auto* native = QGuiApplication::platformNativeInterface();
        auto* display =
            native ? static_cast<wl_display*>(native->nativeResourceForIntegration("display"))
                   : nullptr;
        if (!display) {
            emit failed(QStringLiteral("No Wayland display is available"));
            return false;
        }

        mCallback = wl_display_sync(display);
        if (!mCallback) {
            emit failed(QStringLiteral("Could not queue a Wayland display sync"));
            return false;
        }

        static const wl_callback_listener listener = {
            [](void* data, wl_callback* callback, uint32_t) {
                auto* self = static_cast<WaylandRoundtrip*>(data);
                if (self->mCallback != callback)
                    return;
                self->mCallback = nullptr;
                wl_callback_destroy(callback);
                emit self->pendingChanged();
                emit self->completed();
            },
        };
        wl_callback_add_listener(mCallback, &listener, this);
        emit pendingChanged();

        // EAGAIN merely means Qt's Wayland event loop will flush the queued
        // request when the socket becomes writable. Other errors mean there is
        // no completion proof, so leave the process alive.
        if (wl_display_flush(display) < 0 && errno != EAGAIN) {
            clearCallback();
            emit failed(QStringLiteral("Could not flush the Wayland display sync"));
            return false;
        }
        return true;
    }

  signals:
    void pendingChanged();
    void completed();
    void failed(const QString& message);

  private:
    void clearCallback() {
        if (!mCallback)
            return;
        wl_callback_destroy(mCallback);
        mCallback = nullptr;
        emit pendingChanged();
    }

    wl_callback* mCallback = nullptr;
};

class WaylandSyncPlugin final : public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.qt-project.Qt.QQmlExtensionInterface")

  public:
    void registerTypes(const char* uri) override {
        qmlRegisterType<WaylandRoundtrip>(uri, 1, 0, "WaylandRoundtrip");
    }
};

} // namespace

#include "plugin.moc"
