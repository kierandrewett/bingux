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
        Quickshell.execDetached([Python.python, Python.helper("shell_notify.py"), JSON.stringify(payload)]);
    }
}
