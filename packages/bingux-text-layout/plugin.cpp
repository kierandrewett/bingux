#include <QQmlExtensionPlugin>
#include <qqml.h>
#include "document-spacing.h"

class TextLayoutPlugin : public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QQmlExtensionInterface_iid)
public:
    void registerTypes(const char *uri) override {
        qmlRegisterType<DocumentSpacing>(uri, 1, 0, "DocumentSpacing");
    }
};

#include "plugin.moc"
