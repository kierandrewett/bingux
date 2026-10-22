import QtQuick

// One installer page: a KioskPage that declares its card chrome. The wizard
// reads these properties for the active page, and calls primary() or
// secondary() when the buttons are pressed. Pages register themselves so the
// wizard can find the active one.
KioskPage {
    id: page

    required property var wizard

    property string title: ""
    property string primaryText: "Continue"
    property bool primaryEnabled: true
    property bool primaryVisible: true
    property string secondaryText: ""
    property string secondaryIcon: ""
    property bool secondaryVisible: false
    property bool allowBack: true
    property int bodyHeight: 260

    function primary() {}
    function secondary() {}

    Component.onCompleted: wizard.registerPage(page)
}
