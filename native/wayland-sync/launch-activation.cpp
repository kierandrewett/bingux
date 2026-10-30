#include "launch-activation.h"

#include <QGuiApplication>
#include <QJSValue>
#include <QPointer>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <QtGui/qguiapplication_platform.h>
#include <QtWaylandClient/QWaylandClientExtension>
#include <qpa/qplatformnativeinterface.h>
#include <qqml.h>
#include <wayland-client.h>

#include <cerrno>
#include "qwayland-xdg-activation-v1.h"

namespace {

class ActivationManager : public QWaylandClientExtensionTemplate<ActivationManager>,
                          public QtWayland::xdg_activation_v1 {
    Q_OBJECT
  public:
    ActivationManager() : QWaylandClientExtensionTemplate(1) {
        connect(this, &QWaylandClientExtension::activeChanged, this, [this] {
            if (!isActive() && isInitialized())
                destroy();
        });
        initialize();
    }
    ~ActivationManager() override {
        if (isInitialized())
            destroy();
    }
};

ActivationManager* manager() {
    if (!QGuiApplication::platformName().startsWith("wayland"))
        return nullptr;
    static QPointer<ActivationManager> value;
    if (!value) {
        value = new ActivationManager;
        value->setParent(qApp);
        QObject::connect(qApp, &QCoreApplication::aboutToQuit, value, &QObject::deleteLater);
    }
    return value;
}

void deliver(const QJSValue& callback, const QString& token, const QString& reason) {
    if (callback.call({QJSValue(token), QJSValue(reason)}).isError())
        qWarning("Launch activation callback failed");
}

class TokenRequest : public QObject, public QtWayland::xdg_activation_token_v1 {
  public:
    TokenRequest(::xdg_activation_token_v1* token, const QJSValue& callback, QObject* parent)
        : QObject(parent), QtWayland::xdg_activation_token_v1(token), mCallback(callback) {
        mTimeout.setSingleShot(true);
        connect(&mTimeout, &QTimer::timeout, this,
                [this] { finish({}, QStringLiteral("Activation token request timed out")); });
        mTimeout.start(1500);
    }
    ~TokenRequest() override {
        if (isInitialized())
            destroy();
    }
    void finish(const QString& token, const QString& reason) {
        if (mFinished)
            return;
        mFinished = true;
        mTimeout.stop();
        if (isInitialized())
            destroy();
        const auto callback = mCallback;
        mCallback = {};
        deleteLater();
        deliver(callback, token, reason);
    }
    void xdg_activation_token_v1_done(const QString& token) override {
        finish(token, token.isEmpty() ? QStringLiteral("Compositor returned no activation token")
                                      : QString());
    }

  private:
    QJSValue mCallback;
    QTimer mTimeout;
    bool mFinished = false;
};

class LaunchActivation : public QObject {
    Q_OBJECT
  public:
    explicit LaunchActivation(QObject* parent = nullptr) : QObject(parent) {
        // Bind before the first input callback. No separate Wayland connection
        // can carry the Qt surface and seat that authorize this gesture.
        manager();
    }

    // Call synchronously from a pointer/key handler, before feedback, timers or
    // popup dismissal. No source is passed by IPC callers. A token is a request
    // for compositor authorization, never proof that activation will be allowed.
    Q_INVOKABLE void request(QQuickItem* source, const QJSValue& callback) {
        if (!callback.isCallable())
            return;
        auto* extension = manager();
        auto* native = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>();
        auto* window = source ? source->window() : nullptr;
        if (!extension || !extension->isActive() || !native) {
            deliver(callback, {}, QStringLiteral("Wayland activation is unavailable"));
            return;
        }
        if (!source || !source->isVisible() || !source->isEnabled() || !window ||
            !window->isVisible()) {
            deliver(callback, {}, QStringLiteral("No visible gesture surface"));
            return;
        }
        auto* platform = QGuiApplication::platformNativeInterface();
        auto* surface =
            platform
                ? static_cast<wl_surface*>(platform->nativeResourceForWindow("surface", window))
                : nullptr;
        auto* seat = native->lastInputSeat();
        const auto serial = native->lastInputSerial();
        if (!surface || !seat || !serial) {
            deliver(callback, {}, QStringLiteral("No Wayland input context"));
            return;
        }
        auto* token = new TokenRequest(extension->get_activation_token(), callback, this);
        token->set_surface(surface);
        token->set_serial(serial, seat);
        token->commit();
        // Qt dispatches the reply asynchronously. Do not block the shell or
        // retain tokens for later clicks. Each request has its own timeout.
        if (wl_display_flush(native->display()) < 0 && errno != EAGAIN)
            token->finish({}, QStringLiteral("Could not flush activation request"));
    }
};

} // namespace

void registerLaunchActivation(const char* uri) {
    qmlRegisterType<LaunchActivation>(uri, 1, 0, "LaunchActivation");
}

#include "launch-activation.moc"
