import QtQuick

// The installer flow on one KioskScreen. Pages are children of the stack and
// register themselves; the active page drives the card's title, buttons and
// body height. Install results move the stack to the finished or error page.
KioskScreen {
    id: root

    // InstallerClient, which reports install results.
    property var client: null
    property var pages: []
    readonly property var activePage: {
        for (const page of pages)
            if (page.pageId === stack.current)
                return page;
        return null;
    }

    function registerPage(page) {
        pages = pages.concat([page]);
    }
    function go(pageId) {
        stack.go(pageId);
    }
    function replace(pageId) {
        stack.replace(pageId);
    }
    function goBack() {
        if (root.activePage && root.activePage.allowBack)
            stack.back();
    }
    function primary() {
        const page = root.activePage;
        if (page && page.primaryVisible && page.primaryEnabled)
            page.primary();
    }

    cardTitle: activePage ? activePage.title : ""
    backVisible: activePage ? activePage.allowBack && stack.canGoBack : false
    primaryText: activePage ? activePage.primaryText : ""
    primaryEnabled: activePage ? activePage.primaryEnabled : false
    primaryVisible: activePage ? activePage.primaryVisible : false
    secondaryText: activePage ? activePage.secondaryText : ""
    secondaryIcon: activePage ? activePage.secondaryIcon : ""
    secondaryVisible: activePage ? activePage.secondaryVisible : false
    cardBodyHeight: activePage ? activePage.bodyHeight : 260

    onBackRequested: goBack()
    onPrimaryRequested: primary()
    onSecondaryRequested: {
        if (root.activePage)
            root.activePage.secondary();
    }

    Connections {
        target: root.client
        enabled: root.client !== null
        function onInstallFinished(data) {
            stack.replace("finished");
        }
        function onInstallFailed(code, message) {
            stack.replace("error");
        }
    }

    KioskPageStack {
        id: stack
        anchors.fill: parent
        current: "welcome"

        InstallerWelcomePage {
            wizard: root
            stack: stack
            pageId: "welcome"
        }
        InstallerKeyboardPage {
            wizard: root
            stack: stack
            pageId: "keyboard"
        }
        InstallerNetworkPage {
            wizard: root
            stack: stack
            pageId: "network"
        }
        InstallerTimezonePage {
            wizard: root
            stack: stack
            pageId: "timezone"
        }
        InstallerHostnamePage {
            wizard: root
            stack: stack
            pageId: "hostname"
        }
        InstallerDiskPage {
            wizard: root
            stack: stack
            pageId: "disk"
            client: root.client
        }
        InstallerPartitionsPage {
            wizard: root
            stack: stack
            pageId: "partitions"
        }
        InstallerStoragePage {
            wizard: root
            stack: stack
            pageId: "storage"
        }
        InstallerAccountPage {
            wizard: root
            stack: stack
            pageId: "account"
        }
        InstallerSummaryPage {
            wizard: root
            stack: stack
            pageId: "summary"
            client: root.client
        }
        InstallerProgressPage {
            wizard: root
            stack: stack
            pageId: "progress"
            client: root.client
        }
        InstallerFinishedPage {
            wizard: root
            stack: stack
            pageId: "finished"
        }
        InstallerErrorPage {
            wizard: root
            stack: stack
            pageId: "error"
            client: root.client
        }
    }
}
