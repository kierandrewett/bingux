#include <QGuiApplication>
#include <QPainterPath>
#include <QPlatformSurfaceEvent>
#include <QPointer>
#include <QQuickItem>
#include <QQuickWindow>
#include <QRegion>
#include <QtGui/qguiapplication_platform.h>
#include <QtWaylandClient/QWaylandClientExtension>
#include <qpa/qplatformnativeinterface.h>
#include <qqml.h>
#include <wayland-client.h>
#include "qwayland-ext-background-effect-v1.h"

namespace {
class BackgroundManager : public QWaylandClientExtensionTemplate<BackgroundManager>,
                          public QtWayland::ext_background_effect_manager_v1 {
    Q_OBJECT
public:
    bool blur = false;
    BackgroundManager() : QWaylandClientExtensionTemplate(1) {
        connect(this, &QWaylandClientExtension::activeChanged, this, [this] {
            if (!isActive()) {
                if (isInitialized()) destroy();
                blur = false;
            }
            emit changed();
        });
        initialize();
    }
    ~BackgroundManager() override { if (isInitialized()) destroy(); }
    void ext_background_effect_manager_v1_capabilities(uint32_t flags) override {
        blur = (flags & capability_blur) != 0;
        emit changed();
    }
signals:
    void changed();
};

BackgroundManager *manager() {
    if (!QGuiApplication::platformName().startsWith("wayland")) return nullptr;
    static QPointer<BackgroundManager> value;
    if (!value) {
        value = new BackgroundManager;
        value->setParent(qApp);
        QObject::connect(qApp, &QCoreApplication::aboutToQuit, value, &QObject::deleteLater);
    }
    return value;
}

class BackgroundEffect;
class WindowBackground : public QObject {
    Q_OBJECT
public:
    QVector<BackgroundEffect *> items;
    ext_background_effect_surface_v1 *effect = nullptr;
    wl_surface *surface = nullptr;
    QRegion previous;
    bool sent = false;
    explicit WindowBackground(QQuickWindow *window) : QObject(window) {
        window->installEventFilter(this);
        connect(window, &QQuickWindow::beforeSynchronizing, this,
                [this, window] { synchronise(window); }, Qt::DirectConnection);
        if (auto *extension = manager())
            connect(extension, &BackgroundManager::changed, window, [window] { window->update(); });
    }
    ~WindowBackground() override { release(); }
    void release() {
        if (effect) ext_background_effect_surface_v1_destroy(effect);
        effect = nullptr;
        surface = nullptr;
        sent = false;
    }
    bool eventFilter(QObject *object, QEvent *event) override {
        if (event->type() == QEvent::PlatformSurface &&
            static_cast<QPlatformSurfaceEvent *>(event)->surfaceEventType() == QPlatformSurfaceEvent::SurfaceAboutToBeDestroyed)
            release();
        return QObject::eventFilter(object, event);
    }
    void synchronise(QQuickWindow *window);
};

class BackgroundEffect : public QObject {
    Q_OBJECT
    Q_PROPERTY(QQuickItem *target READ target WRITE setTarget NOTIFY targetChanged)
    Q_PROPERTY(qreal radius READ radius WRITE setRadius NOTIFY regionChanged)
    Q_PROPERTY(bool enabled READ enabled WRITE setEnabled NOTIFY regionChanged)
    Q_PROPERTY(bool available READ available NOTIFY availableChanged)
public:
    QPointer<QQuickItem> item;
    QPointer<WindowBackground> owner;
    QMetaObject::Connection windowChanged;
    QMetaObject::Connection destroyed;
    qreal cornerRadius = 0;
    bool requested = true;
    explicit BackgroundEffect(QObject *parent = nullptr) : QObject(parent) {
        if (auto *extension = manager())
            connect(extension, &BackgroundManager::changed, this, &BackgroundEffect::availableChanged);
    }
    ~BackgroundEffect() override { detach(); }
    QQuickItem *target() const { return item; }
    qreal radius() const { return cornerRadius; }
    bool enabled() const { return requested; }
    bool available() const { auto *m = manager(); return m && m->isActive() && m->blur; }
    void update() { if (item && item->window()) item->window()->update(); }
    void setRadius(qreal value) {
        value = qMax(qreal(0), value);
        if (cornerRadius == value) return;
        cornerRadius = value; update(); emit regionChanged();
    }
    void setEnabled(bool value) {
        if (requested == value) return;
        requested = value; update(); emit regionChanged();
    }
    void detach() {
        if (owner) {
            owner->items.removeAll(this);
            if (auto *window = qobject_cast<QQuickWindow *>(owner->parent())) window->update();
        }
        owner = nullptr;
    }
    void attach(QQuickWindow *window) {
        detach();
        if (!window) return;
        auto *group = window->findChild<WindowBackground *>("binguxBackgroundEffect", Qt::FindDirectChildrenOnly);
        if (!group) {
            group = new WindowBackground(window);
            group->setObjectName("binguxBackgroundEffect");
        }
        owner = group;
        owner->items.append(this);
        window->update();
    }
    void setTarget(QQuickItem *target) {
        if (item == target) return;
        disconnect(windowChanged); disconnect(destroyed); detach();
        item = target;
        if (item) {
            windowChanged = connect(item, &QQuickItem::windowChanged, this,
                [this](QQuickWindow *window) { attach(window); });
            destroyed = connect(item, &QObject::destroyed, this, [this] { detach(); });
            attach(item->window());
        }
        emit targetChanged();
    }
signals:
    void targetChanged();
    void regionChanged();
    void availableChanged();
};

void WindowBackground::synchronise(QQuickWindow *window) {
    auto *extension = manager();
    auto *native = qGuiApp->nativeInterface<QNativeInterface::QWaylandApplication>();
    if (!extension || !extension->isActive() || !extension->blur || !native) return;
    auto *current = static_cast<wl_surface *>(QGuiApplication::platformNativeInterface()
        ->nativeResourceForWindow("surface", window));
    if (!current) return;
    if (surface != current) {
        release();
        surface = current;
        effect = extension->get_background_effect(surface);
    }
    QRegion region;
    // This callback runs while Qt blocks the GUI thread for scene synchronisation.
    // Regions and buffers therefore describe the same item geometry and frame.
    for (auto *entry : items) {
        auto *item = entry->item.data();
        if (!entry->enabled() || !item || !item->isVisible() || item->width() <= 0 || item->height() <= 0) continue;
        qreal opacity = 1;
        for (auto *parent = item; parent; parent = parent->parentItem()) opacity *= parent->opacity();
        if (opacity <= 0) continue;
        QPainterPath path;
        qreal radius = qMin(entry->radius(), qMin(item->width(), item->height()) / 2);
        path.addRoundedRect(item->boundingRect(), radius, radius);
        bool valid;
        QTransform transform = item->itemTransform(window->contentItem(), &valid);
        if (!valid) continue;
        QRegion shape(transform.map(path).toFillPolygon().toPolygon());
        for (auto *parent = item->parentItem(); parent; parent = parent->parentItem())
            if (parent->clip()) shape &= parent->mapRectToScene(parent->boundingRect()).toAlignedRect();
        region |= shape;
    }
    region &= QRect(0, 0, window->width(), window->height());
    region.translate(window->frameMargins().left(), window->frameMargins().top());
    if (sent && region == previous) return;
    wl_region *wire = nullptr;
    if (!region.isEmpty()) {
        wire = wl_compositor_create_region(native->compositor());
        for (const QRect &rect : region)
            wl_region_add(wire, rect.x(), rect.y(), rect.width(), rect.height());
    }
    ext_background_effect_surface_v1_set_blur_region(effect, wire);
    if (wire) wl_region_destroy(wire);
    previous = region;
    sent = true;
}
} // namespace

void registerBackgroundEffect(const char *uri) {
    qmlRegisterType<BackgroundEffect>(uri, 1, 0, "BackgroundEffect");
}
#include "background-effect.moc"
