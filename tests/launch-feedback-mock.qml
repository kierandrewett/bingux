pragma Singleton
import QtQuick

QtObject {
    property int sequence: 0
    property var active: []
    function begin(application, callback) {
        const token = String(++sequence);
        active = active.concat([
            {
                token: token,
                application: application
            }
        ]);
        if (callback)
            Qt.callLater(callback);
        return token;
    }
    function end(token) {
        active = active.filter(entry => entry.token !== token);
    }
}
