#include <QGuiApplication>
#include <QQmlExtensionPlugin>
#include <qqml.h>
#include <QWindow>
#include <QPointer>
#include <QTimer>
#include <QRegion>
#include <qpa/qplatformwindow_p.h>

class WindowFrame : public QObject {
    Q_OBJECT
    Q_PROPERTY(QWindow* window READ window WRITE setWindow NOTIFY changed)
    Q_PROPERTY(int margin READ margin WRITE setMargin NOTIFY changed)
  public:
    using QObject::QObject;
    QWindow* window() const {
        return m_window;
    }
    int margin() const {
        return m_margin;
    }
    void setWindow(QWindow* window) {
        m_window = window;
        if (window) {
            connect(window, &QWindow::visibleChanged, this, &WindowFrame::schedule);
            connect(window, &QWindow::widthChanged, this, &WindowFrame::schedule);
            connect(window, &QWindow::heightChanged, this, &WindowFrame::schedule);
        }
        schedule();
        emit changed();
    }
    void setMargin(int value) {
        if (m_margin == value)
            return;
        m_margin = value;
        // Qt exposes the requested maximised state before Mutter configures the
        // new size. Changing native margins here commits an intermediate resize
        // and consumes Mutter's size-change animation. Apply after configure,
        // through widthChanged/heightChanged, except for initial setup.
        if (m_appliedMargin < 0)
            schedule();
        emit changed();
    }
  signals:
    void changed();

  private:
    void schedule() {
        QTimer::singleShot(0, this, [this] {
            if (!m_window || !m_window->isVisible())
                return;
            if (m_appliedMargin != m_margin) {
                if (auto* native =
                        m_window->nativeInterface<QNativeInterface::Private::QWaylandWindow>()) {
                    m_appliedMargin = m_margin;
                    native->setCustomMargins(QMargins(m_margin, m_margin, m_margin, m_margin));
                }
            }
            m_window->setMask(QRegion(QRect(QPoint(0, 0), m_window->size())
                                          .adjusted(m_margin, m_margin, -m_margin, -m_margin)));
        });
    }
    QPointer<QWindow> m_window;
    int m_margin = 0;
    int m_appliedMargin = -1;
};

class SettingsPlatform final : public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.qt-project.Qt.QQmlExtensionInterface")
  public:
    void registerTypes(const char* uri) override {
        qmlRegisterType<WindowFrame>(uri, 1, 0, "WindowFrame");
    }
    void initializeEngine(QQmlEngine*, const char*) override {
        QGuiApplication::setApplicationName(QStringLiteral("Bingux Settings"));
        QGuiApplication::setApplicationDisplayName(QStringLiteral("Bingux Settings"));
        QGuiApplication::setDesktopFileName(QStringLiteral("bingux-settings"));
    }
};
#include "plugin.moc"
