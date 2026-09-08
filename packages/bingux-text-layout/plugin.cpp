#include <QQmlExtensionPlugin>
#include <qqml.h>
#include "document-edit.h"

class TextLayoutPlugin : public QQmlExtensionPlugin {
    Q_OBJECT
    Q_PLUGIN_METADATA(IID QQmlExtensionInterface_iid)
public:
    void registerTypes(const char *uri) override {
        qmlRegisterType<DocumentEdit>(uri, 1, 0, "DocumentEdit");
        qmlRegisterType<DocumentSpacing>(uri, 1, 0, "DocumentSpacing");
    }
};

#include "plugin.moc"
