#include <QGuiApplication>
#include <QPointer>
#include <QPlatformSurfaceEvent>
#include <QQmlExtensionPlugin>
#include <QQuickItem>
#include <QQuickWindow>
#include <QVector>
#include <qpa/qplatformnativeinterface.h>
#include <qqml.h>
#include <wayland-client.h>
#include <cmath>
#include <cstring>
#include "blur-fade-client.h"

namespace {

struct Connection {
    wl_display *display = nullptr;
    wl_registry *registry = nullptr;
    wl_event_queue *queue = nullptr;
    gnoblin_blur_fade_manager_v1 *manager = nullptr;

    Connection() {
        if (!QGuiApplication::platformName().startsWith("wayland")) return;
        auto *native = QGuiApplication::platformNativeInterface();
        if (!native) return;
        display = static_cast<wl_display *>(native->nativeResourceForIntegration("display"));
        if (!display) return;
        queue = wl_display_create_queue(display);
        registry = wl_display_get_registry(display);
        wl_proxy_set_queue(reinterpret_cast<wl_proxy *>(registry), queue);
        static const wl_registry_listener listener = {
            [](void *data, wl_registry *registry, uint32_t name, const char *interface, uint32_t) {
                auto *self = static_cast<Connection *>(data);
                if (std::strcmp(interface, "gnoblin_blur_fade_manager_v1") == 0)
                    self->manager = static_cast<gnoblin_blur_fade_manager_v1 *>(wl_registry_bind(
                        registry, name, &gnoblin_blur_fade_manager_v1_interface, 1));
            },
            [](void *, wl_registry *, uint32_t) {},
        };
        wl_registry_add_listener(registry, &listener, this);
        wl_display_roundtrip_queue(display, queue);
        QObject::connect(qApp, &QCoreApplication::aboutToQuit, qApp, [this] {
            if (manager) gnoblin_blur_fade_manager_v1_destroy(manager);
            wl_registry_destroy(registry);
            wl_event_queue_destroy(queue);
            manager = nullptr;
        });
    }
};

Connection &connection() {
    // The Wayland connection belongs to Qt. Release our proxies before Qt's
    // platform integration is destroyed, not from a static destructor.
    static auto *value = new Connection;
    return *value;
}

class SurfaceFade;

class WindowFades : public QObject {
    Q_OBJECT
public:
    QVector<SurfaceFade *> items;
    QVector<int32_t> previous;
    wl_surface *previousSurface = nullptr;
    bool capacityWarning = false;

    explicit WindowFades(QQuickWindow *window) : QObject(window) {
        connection();
        window->installEventFilter(this);
        QObject::connect(window, &QQuickWindow::beforeSynchronizing, this,
            [this, window] { synchronise(window); }, Qt::DirectConnection);
    }
    bool eventFilter(QObject *object, QEvent *event) override {
        if (event->type() == QEvent::PlatformSurface) {
            previousSurface = nullptr;
            previous.clear();
        }
        return QObject::eventFilter(object, event);
    }
    void synchronise(QQuickWindow *window);
};

class SurfaceFade : public QObject {
    Q_OBJECT
    Q_PROPERTY(QQuickItem *target READ target WRITE setTarget NOTIFY targetChanged)
    Q_PROPERTY(bool available READ available CONSTANT)
public:
    QPointer<QQuickItem> item;
    QPointer<WindowFades> owner;
    QMetaObject::Connection windowChanged;
    QMetaObject::Connection destroyed;

    explicit SurfaceFade(QObject *parent = nullptr) : QObject(parent) { connection(); }
    ~SurfaceFade() override { detach(); }
    QQuickItem *target() const { return item; }
    bool available() const { return connection().manager != nullptr; }
    void detach() {
        if (owner) {
            owner->items.removeAll(this);
            if (auto *window = qobject_cast<QQuickWindow *>(owner->parent())) window->update();
        }
        owner = nullptr;
    }
    void attach(QQuickWindow *window) {
        detach();
        if (!window || !available()) return;
        auto *group = window->findChild<WindowFades *>("binguxBufferFades", Qt::FindDirectChildrenOnly);
        if (!group) {
            group = new WindowFades(window);
            group->setObjectName("binguxBufferFades");
        }
        owner = group;
        group->items.append(this);
        window->update();
    }
    void setTarget(QQuickItem *target) {
        if (item == target) return;
        QObject::disconnect(windowChanged);
        QObject::disconnect(destroyed);
        detach();
        item = target;
        if (target) {
            windowChanged = QObject::connect(target, &QQuickItem::windowChanged, this,
                [this](QQuickWindow *window) { attach(window); });
            destroyed = QObject::connect(target, &QObject::destroyed, this, [this] { detach(); });
            attach(target->window());
        }
        emit targetChanged();
    }
signals:
    void targetChanged();
};

void WindowFades::synchronise(QQuickWindow *window) {
    auto *manager = connection().manager;
    if (!manager) return;
    auto *surface = static_cast<wl_surface *>(QGuiApplication::platformNativeInterface()
        ->nativeResourceForWindow("surface", window));
    if (!surface) return;
    QVector<int32_t> values;
    bool fading = false;
    // Qt blocks the GUI thread during scene graph synchronisation. Read the
    // same item state that this frame will render, then marshal metadata before
    // the platform's buffer commit. No separate socket or animation timer is used.
    for (auto *entry : items) {
        auto *item = entry->target();
        if (!item || !item->isVisible() || item->width() <= 0 || item->height() <= 0) continue;
        auto rect = item->mapRectToScene(item->boundingRect());
        for (auto *parent = item->parentItem(); parent; parent = parent->parentItem())
            if (parent->clip()) rect = rect.intersected(parent->mapRectToScene(parent->boundingRect()));
        rect = rect.intersected(QRectF(0, 0, window->width(), window->height()));
        if (rect.isEmpty()) continue;
        if (values.size() == 32 * 5) {
            if (!capacityWarning) qWarning("Bingux.Effects: more than 32 visible fade regions in one window");
            capacityWarning = true;
            break;
        }
        qreal opacity = 1;
        for (auto *parent = item; parent; parent = parent->parentItem()) opacity *= parent->opacity();
        fading = fading || opacity < 1;
        rect.adjust(-2, -2, 2, 2);
        // Qt's client-side decorations share wl_surface with the Quick scene.
        // Scene coordinates start below/inside those decoration margins.
        rect.translate(window->frameMargins().left(), window->frameMargins().top());
        for (qreal value : {rect.x(), rect.y(), rect.width(), rect.height(), opacity})
            values.append(static_cast<int32_t>(std::round(qBound(-65536.0, value, 65536.0) * 256)));
    }
    // Settled windows use the existing fast mask. Retain opaque regions only
    // while another item fades, so overlapping cards keep their coverage.
    if (!fading) values.clear();
    if (surface == previousSurface && values == previous) return;
    wl_array array = {static_cast<size_t>(values.size() * sizeof(int32_t)), 0, values.data()};
    gnoblin_blur_fade_manager_v1_set_fades(manager, surface, &array);
    previous = values;
    previousSurface = surface;
}

} // namespace

class EffectsPlugin : public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.qt-project.Qt.QQmlExtensionInterface")
public:
    void registerTypes(const char *uri) override {
        qmlRegisterType<SurfaceFade>(uri, 1, 0, "SurfaceFade");
    }
};

#include "plugin.moc"
