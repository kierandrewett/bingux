pragma Singleton
import QtQuick
import Quickshell

QtObject {
    function send(title, body, options) {
        const payload = Object.assign({
            title: title,
            body: body,
            icon: "dialog-error-symbolic"
        }, options || {});
        Quickshell.execDetached([Python.python, decodeURIComponent(Qt.resolvedUrl("shell_notify.py").toString().replace(/^file:\/\//, "")), JSON.stringify(payload)]);
    }
}
