import QtQuick
import QtQuick.Window
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell

ShellPopup {
    id: root
    property bool inlineMode: false
    property bool calendarServiceEnabled: true
    keyboardInteractive: !inlineMode
    dismissOnOutsideClick: !inlineMode
    function focusCalendar() {
        grid.forceActiveFocus();
    }
    // 0: days, 1: months in a year, 2: years in a decade, 3: decades in a century.
    property int viewLevel: 0
    property int previousPeriodLevel: 0
    property int previousPeriodYear: displayedMonth.getFullYear()
    property date previousPeriodMonth: displayedMonth
    property date previousPeriodSelection: selectedDate
    property real periodProgress: 1
    property int periodDirection: 1
    function changePeriod(level, year) {
        // Day-to-day navigation has its own month transition. Returning to today
        // must not restart the hierarchy transition within the same view.
        if (level === viewLevel && (level === 0 || year === browseYear)) {
            browseYear = year;
            return;
        }
        periodMotion.stop();
        previousPeriodLevel = viewLevel;
        previousPeriodYear = browseYear;
        previousPeriodMonth = renderedMonth;
        previousPeriodSelection = selectedDate;
        periodDirection = level === viewLevel ? (year >= browseYear ? 1 : -1) : (level > viewLevel ? 1 : -1);
        viewLevel = level;
        browseYear = year;
        if (!visible || preparing || Theme.reducedMotion)
            periodProgress = 1;
        else {
            periodProgress = 0;
            periodMotion.start();
        }
    }
    function backPeriod() {
        changePeriod(Math.max(0, viewLevel - 1), browseYear);
        Qt.callLater(() => viewLevel === 0 ? grid.forceActiveFocus() : periodPicker.focusSelection());
    }
    NumberAnimation {
        id: periodMotion
        target: root
        property: "periodProgress"
        to: 1
        duration: 240
        easing.type: Easing.OutCubic
    }
    property int browseYear: displayedMonth.getFullYear()
    readonly property string periodTitle: viewLevel === 0 ? renderedMonth.toLocaleDateString(Qt.locale(), "MMMM yyyy") : viewLevel === 1 ? String(browseYear) : viewLevel === 2 ? (Math.floor(browseYear / 10) * 10) + "–" + (Math.floor(browseYear / 10) * 10 + 9) : (Math.floor(browseYear / 100) * 100) + "–" + (Math.floor(browseYear / 100) * 100 + 99)
    function showBroaderPeriod() {
        changePeriod(Math.min(3, viewLevel + 1), viewLevel === 0 ? displayedMonth.getFullYear() : browseYear);
        Qt.callLater(() => periodPicker.focusSelection());
    }
    function shiftPeriod(direction) {
        if (viewLevel === 0)
            shiftMonth(direction);
        else
            changePeriod(viewLevel, Math.max(1, Math.min(9999, browseYear + direction * (viewLevel === 1 ? 1 : viewLevel === 2 ? 10 : 100))));
    }
    function choosePeriod(year, month) {
        if (viewLevel === 1) {
            const date = new Date(2000, month, 1);
            date.setFullYear(year);
            const last = new Date(date);
            last.setMonth(month + 1);
            last.setDate(0);
            date.setDate(Math.min(selectedDate.getDate(), last.getDate()));
            changePeriod(0, year);
            selectDate(date);
            Qt.callLater(() => grid.forceActiveFocus(Qt.OtherFocusReason));
        } else {
            changePeriod(viewLevel - 1, year);
            Qt.callLater(() => periodPicker.focusSelection());
        }
    }
    property date today: new Date()
    property date selectedDate: today
    property var highlightedDate: null
    property date displayedMonth: new Date(today.getFullYear(), today.getMonth(), 1)
    readonly property var dayEvents: eventSource.forDay(selectedDate)
    property var eventSource: calendarEvents
    property real monthProgress: 1
    property real agendaProgress: 1
    property int monthDirection: 1
    property bool preparing: false
    property date renderedMonth: displayedMonth
    property date renderedSelection: selectedDate
    property date previousMonth: displayedMonth
    property date previousSelection: selectedDate
    property var previousEvents: []
    property var presentedEvents: []
    property date agendaDate: selectedDate
    popupWidth: 384
    contentPadding: 16
    surfaceColor: Theme.popupSurface
    popupHeight: calendar.implicitHeight + contentPadding * 2
    onAboutToOpen: {
        if (inlineMode)
            return;
        calendarViewport.contentY = 0;
        preparing = true;
        viewLevel = 0;
        periodMotion.stop();
        periodProgress = 1;
        highlightedDate = null;
        today = new Date();
        selectDate(today);
        monthMotion.stop();
        agendaMotion.stop();
        presentMonth();
        syncAgenda();
        monthProgress = 1;
        agendaProgress = 1;
        preparing = false;
        Qt.callLater(() => {
            if (root.visible)
                grid.forceActiveFocus(Qt.OtherFocusReason);
        });
    }
    Timer {
        interval: 30000
        repeat: true
        running: root.visible
        onTriggered: {
            const now = new Date();
            if (root.sameDay(now, root.today))
                return;
            const followingToday = root.sameDay(root.selectedDate, root.today);
            root.today = now;
            if (followingToday)
                root.selectDate(now);
        }
    }
    NumberAnimation {
        id: monthMotion
        target: root
        property: "monthProgress"
        to: 1
        duration: Theme.calendarPageMotion
        easing.type: Easing.OutCubic
        onFinished: root.presentMonth()
    }
    SequentialAnimation {
        id: agendaMotion
        NumberAnimation {
            target: root
            property: "agendaProgress"
            to: 0
            duration: Theme.reducedMotion ? 0 : 60
            easing.type: Easing.OutQuad
        }
        ScriptAction {
            script: root.syncAgenda()
        }
        NumberAnimation {
            target: root
            property: "agendaProgress"
            to: 1
            duration: Theme.reducedMotion ? 0 : 120
            easing.type: Easing.OutCubic
        }
    }
    // The pending refresh belongs to this view and is cancelled with it.
    Timer {
        id: agendaUpdate
        interval: 0
        onTriggered: root.updateAgenda()
    }
    onDayEventsChanged: agendaUpdate.restart()
    function syncAgenda() {
        presentedEvents = dayEvents;
        agendaDate = selectedDate;
        agendaScroll.stop();
        agenda.contentY = 0;
    }
    function updateAgenda() {
        if (!visible || preparing || Theme.reducedMotion || (presentedEvents.length === 0 && dayEvents.length === 0)) {
            agendaMotion.stop();
            syncAgenda();
            agendaProgress = 1;
        } else
            agendaMotion.restart();
    }
    function sameMonth(a, b) {
        return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth();
    }
    function presentMonth() {
        if (preparing || !visible || Theme.reducedMotion) {
            monthMotion.stop();
            renderedMonth = displayedMonth;
            renderedSelection = selectedDate;
            monthProgress = 1;
            return;
        }
        if (sameMonth(displayedMonth, renderedMonth)) {
            renderedSelection = selectedDate;
            return;
        }
        if (monthMotion.running) {
            // Reversing retraces the current movement instead of snapping to a page.
            if (sameMonth(displayedMonth, previousMonth)) {
                monthMotion.stop();
                const outgoing = renderedMonth, outgoingSelection = renderedSelection;
                renderedMonth = previousMonth;
                previousMonth = outgoing;
                renderedSelection = selectedDate;
                previousSelection = outgoingSelection;
                monthDirection *= -1;
                monthProgress = 1 - monthProgress;
                monthMotion.duration = Theme.calendarPageMotion * (1 - monthProgress);
                monthMotion.start();
            }
            // Further clicks coalesce to the newest requested month after this page settles.
            return;
        }
        previousMonth = renderedMonth;
        previousSelection = renderedSelection;
        previousEvents = eventSource.events;
        // Disable cell highlight tweens before MonthGrid reuses its delegates.
        monthProgress = 0;
        renderedMonth = displayedMonth;
        renderedSelection = selectedDate;
        monthDirection = renderedMonth > previousMonth ? 1 : -1;
        monthMotion.duration = Theme.calendarPageMotion;
        monthMotion.start();
    }
    function sameDay(a, b) {
        return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
    }
    function selectDate(date) {
        if (sameDay(date, selectedDate))
            return;
        selectedDate = date;
        displayedMonth = new Date(date.getFullYear(), date.getMonth(), 1);
        presentMonth();
    }
    function chooseDate(date) {
        highlightedDate = date;
        selectDate(date);
    }
    function shiftMonth(amount) {
        const next = new Date(displayedMonth.getFullYear(), displayedMonth.getMonth() + amount, 1);
        const last = new Date(next.getFullYear(), next.getMonth() + 1, 0).getDate();
        selectDate(new Date(next.getFullYear(), next.getMonth(), Math.min(selectedDate.getDate(), last)));
    }
    property var launchCalendar: (command, prepareCommand) => calendarLauncher.launch(command, prepareCommand)
    signal applicationLaunchStarted(string desktopId)
    ApplicationLauncher {
        id: calendarLauncher
        desktopId: "org.gnome.Calendar"
        onLaunchStarted: desktopId => root.applicationLaunchStarted(desktopId)
        onFailed: message => {
            root.eventSource.error = message;
            root.visible = true;
        }
    }
    function eventCommand(event) {
        const command = ["python3", decodeURIComponent(Qt.resolvedUrl("calendar-launch.py").toString().replace(/^file:\/\//, "")), "--date", new Date(event.start * 1000).toLocaleDateString(Qt.locale(), "yyyy-MM-dd")];
        const parts = String(event.id || "").split("\n");
        if (parts.length === 3 && parts[0] && parts[1]) {
            if (!parts[2])
                parts.pop();
            command.push("--uuid", parts.join(":"));
        }
        return command;
    }
    function openEvent(event) {
        if (!inlineMode)
            visible = false;
        launchCalendar(eventCommand(event), []);
    }
    function eventTime(event, day) {
        const date = day || selectedDate;
        const start = new Date(event.start * 1000), end = new Date(event.end * 1000);
        const dayStart = new Date(date.getFullYear(), date.getMonth(), date.getDate());
        const dayEnd = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1);
        if (start <= dayStart && end >= dayEnd)
            return "All day";
        return (start < dayStart ? "Earlier" : start.toLocaleTimeString(Qt.locale(), "hh:mm")) + " – " + (end > dayEnd ? "Later" : end.toLocaleTimeString(Qt.locale(), "hh:mm"));
    }
    CalendarEvents {
        id: calendarEvents
        active: root.visible && root.calendarServiceEnabled && root.eventSource === calendarEvents
        month: root.displayedMonth
    }
    Flickable {
        id: calendarViewport
        objectName: "calendarViewport"
        width: parent.width
        height: parent.height
        contentWidth: width
        contentHeight: calendar.height
        clip: true
        interactive: !root.inlineMode && contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        onHeightChanged: contentY = Math.max(0, Math.min(contentY, contentHeight - height))
        function reveal(item) {
            if (root.inlineMode || !interactive)
                return;
            const top = item.mapToItem(contentItem, 0, 0).y;
            const bottom = top + item.height;
            const next = top < contentY ? top : bottom > contentY + height ? bottom - height : contentY;
            contentY = Math.max(0, Math.min(next, contentHeight - height));
        }
        Connections {
            target: calendarViewport.Window.window
            function onActiveFocusItemChanged() {
                const item = target.activeFocusItem;
                for (let parent = item; parent; parent = parent.parent) {
                    if (parent === calendarViewport.contentItem) {
                        calendarViewport.reveal(item);
                        return;
                    }
                }
            }
        }
        ScrollBar.vertical: ScrollBar {
            policy: root.inlineMode ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded
        }
        ColumnLayout {
            id: calendar
            width: calendarViewport.width
            height: root.inlineMode ? implicitHeight : Math.max(calendarViewport.height, dateHeading.implicitHeight + monthCard.implicitHeight + agendaHeading.Layout.minimumHeight + agendaCard.Layout.minimumHeight + spacing * 3)
            spacing: Theme.padding
            RowLayout {
                id: dateHeading
                Layout.minimumHeight: implicitHeight
                Layout.fillWidth: true
                spacing: Theme.gap
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: Theme.spaceSmall
                    Text {
                        text: root.today.toLocaleDateString(Qt.locale(), "dddd")
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                    Text {
                        text: root.today.toLocaleDateString(Qt.locale(), root.popupWidth < 220 ? "d MMM" : "d MMMM")
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: root.popupWidth < 260 ? 20 : 24
                        font.weight: Font.DemiBold
                    }
                }
                Item {
                    Layout.fillWidth: true
                }
                IconButton {
                    visible: root.popupWidth >= 240
                    Layout.fillWidth: false
                    iconName: "x-office-calendar-symbolic"
                    label: "Open Calendar"
                    onClicked: {
                        if (!root.inlineMode)
                            root.visible = false;
                        root.launchCalendar(["gnome-calendar"], []);
                    }
                }
            }
            Rectangle {
                id: monthCard
                Layout.fillWidth: true
                Layout.minimumHeight: implicitHeight
                implicitHeight: monthContents.implicitHeight + Theme.padding * 2
                color: Theme.elevated
                radius: Theme.menuWidgetRadius
                clip: true
                ColumnLayout {
                    id: monthContents
                    Keys.onEscapePressed: event => {
                        if (root.viewLevel === 0) {
                            event.accepted = false;
                            return;
                        }
                        root.backPeriod();
                    }
                    anchors {
                        left: parent.left
                        right: parent.right
                        top: parent.top
                        margins: Theme.padding
                    }
                    spacing: Theme.gap
                    GridLayout {
                        id: monthHeader
                        readonly property bool narrow: root.popupWidth < 300
                        columns: narrow ? 3 : 5
                        columnSpacing: 4
                        rowSpacing: 4
                        Layout.fillWidth: true
                        ActionButton {
                            objectName: "calendarPeriodHeading"
                            Layout.columnSpan: monthHeader.narrow ? 3 : 1
                            Layout.maximumWidth: monthHeader.width
                            Layout.fillWidth: false
                            implicitHeight: 32
                            flat: true
                            hoverEnabled: true
                            alignLeft: true
                            // Keep the button inside the shared content inset, with
                            // one even padding layer around its label.
                            leftPadding: 0
                            rightPadding: 0
                            horizontalPadding: 8
                            text: root.periodTitle
                            enabled: root.viewLevel < 3
                            Accessible.description: root.viewLevel === 0 ? "Choose a month" : root.viewLevel === 1 ? "Choose a year" : "Choose a decade"
                            onClicked: root.showBroaderPeriod()
                        }
                        Item {
                            visible: !monthHeader.narrow
                            Layout.fillWidth: true
                        }
                        ActionButton {
                            objectName: "backToToday"
                            text: "Today"
                            implicitHeight: 32
                            horizontalPadding: 8
                            enabled: root.viewLevel > 0 || !root.sameDay(root.selectedDate, root.today)
                            opacity: enabled ? 1 : 0.45
                            Behavior on opacity {
                                NumberAnimation {
                                    duration: Theme.reducedMotion ? 0 : Theme.motion
                                }
                            }
                            Accessible.description: "Return to today's date. Home key in the calendar."
                            onClicked: {
                                root.changePeriod(0, root.today.getFullYear());
                                root.selectDate(root.today);
                                grid.forceActiveFocus();
                            }
                        }
                        IconButton {
                            objectName: "previousMonth"
                            iconName: "go-previous-symbolic"
                            label: root.viewLevel === 0 ? "Previous month" : root.viewLevel === 1 ? "Previous year" : root.viewLevel === 2 ? "Previous decade" : "Previous century"
                            onClicked: root.shiftPeriod(-1)
                        }
                        IconButton {
                            objectName: "nextMonth"
                            iconName: "go-next-symbolic"
                            label: root.viewLevel === 0 ? "Next month" : root.viewLevel === 1 ? "Next year" : root.viewLevel === 2 ? "Next decade" : "Next century"
                            onClicked: root.shiftPeriod(1)
                        }
                        WheelHandler {
                            target: null
                            property real accumulated: 0
                            onWheel: event => {
                                accumulated += event.angleDelta.y;
                                if (Math.abs(accumulated) >= 120) {
                                    root.shiftPeriod(accumulated > 0 ? -1 : 1);
                                    accumulated = 0;
                                }
                                event.accepted = true;
                            }
                        }
                    }
                    Item {
                        id: monthViewport
                        Layout.fillWidth: true
                        Layout.preferredHeight: 240 + weekdayNames.implicitHeight + Theme.gap
                        clip: true
                        DayOfWeekRow {
                            id: weekdayNames
                            width: parent.width
                            locale: Qt.locale()
                            opacity: root.viewLevel === 0 ? root.periodProgress : root.previousPeriodLevel === 0 ? 1 - root.periodProgress : 0
                            delegate: Text {
                                required property string shortName
                                text: shortName
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                horizontalAlignment: Text.AlignHCenter
                            }
                        }
                        CalendarPeriodPicker {
                            anchors.fill: parent
                            enabled: false
                            visible: root.periodProgress < 1 && root.previousPeriodLevel > 0
                            level: root.previousPeriodLevel
                            year: root.previousPeriodYear
                            selectedDate: root.previousPeriodSelection
                            opacity: 1 - root.periodProgress
                            scale: 1 - root.periodDirection * 0.06 * root.periodProgress
                            transform: Translate {
                                y: -root.periodDirection * 12 * root.periodProgress
                            }
                        }
                        CalendarMonthGrid {
                            width: parent.width
                            height: 240
                            y: weekdayNames.implicitHeight + Theme.gap
                            enabled: false
                            visible: root.periodProgress < 1 && root.previousPeriodLevel === 0
                            pageDate: root.previousPeriodMonth
                            selectedDate: root.previousPeriodSelection
                            highlightedDate: root.highlightedDate
                            eventSource: root.eventSource
                            animateSelection: false
                            opacity: 1 - root.periodProgress
                            scale: 1 - root.periodDirection * 0.06 * root.periodProgress
                            transform: Translate {
                                y: -root.periodDirection * 12 * root.periodProgress
                            }
                        }
                        CalendarPeriodPicker {
                            id: periodPicker
                            objectName: "calendarPeriodPicker"
                            anchors.fill: parent
                            visible: root.viewLevel > 0
                            opacity: root.periodProgress
                            scale: 1 + root.periodDirection * 0.06 * (1 - root.periodProgress)
                            transform: Translate {
                                y: root.periodDirection * 12 * (1 - root.periodProgress)
                            }
                            level: root.viewLevel
                            year: root.browseYear
                            selectedDate: root.selectedDate
                            onChosen: (year, month) => root.choosePeriod(year, month)
                            onPageRequested: direction => root.shiftPeriod(direction)
                            onBackRequested: root.backPeriod()
                        }
                        CalendarMonthGrid {
                            id: outgoingGrid
                            objectName: "outgoingCalendarGrid"
                            width: parent.width
                            height: 240
                            y: weekdayNames.implicitHeight + Theme.gap
                            x: -root.monthDirection * width * root.monthProgress
                            opacity: 1 - root.monthProgress * 0.25
                            visible: root.viewLevel === 0 && root.monthProgress < 1
                            enabled: false
                            animateSelection: false
                            pageDate: root.previousMonth
                            selectedDate: root.previousSelection
                            highlightedDate: root.highlightedDate
                            eventSource: root.eventSource
                            events: root.previousEvents
                        }
                        CalendarMonthGrid {
                            id: grid
                            visible: root.viewLevel === 0
                            enabled: visible
                            objectName: "calendarGrid"
                            width: parent.width
                            height: 240
                            y: weekdayNames.implicitHeight + Theme.gap
                            scale: 1 + root.periodDirection * 0.06 * (1 - root.periodProgress)
                            transform: Translate {
                                y: root.periodDirection * 12 * (1 - root.periodProgress)
                            }
                            x: root.monthDirection * width * (1 - root.monthProgress)
                            opacity: (0.75 + root.monthProgress * 0.25) * root.periodProgress
                            pageDate: root.renderedMonth
                            selectedDate: root.renderedSelection
                            highlightedDate: root.highlightedDate
                            eventSource: root.eventSource
                            animateSelection: root.monthProgress === 1
                            onClicked: date => {
                                root.chooseDate(date);
                                grid.forceActiveFocus(Qt.MouseFocusReason);
                            }
                            activeFocusOnTab: true
                            Keys.onLeftPressed: root.selectDate(new Date(root.selectedDate.getFullYear(), root.selectedDate.getMonth(), root.selectedDate.getDate() - 1))
                            Keys.onRightPressed: root.selectDate(new Date(root.selectedDate.getFullYear(), root.selectedDate.getMonth(), root.selectedDate.getDate() + 1))
                            Keys.onUpPressed: root.selectDate(new Date(root.selectedDate.getFullYear(), root.selectedDate.getMonth(), root.selectedDate.getDate() - 7))
                            Keys.onDownPressed: root.selectDate(new Date(root.selectedDate.getFullYear(), root.selectedDate.getMonth(), root.selectedDate.getDate() + 7))
                            Keys.onPressed: event => {
                                if (event.key === Qt.Key_Home) {
                                    root.selectDate(root.today);
                                    event.accepted = true;
                                } else if (event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown) {
                                    root.shiftMonth((event.key === Qt.Key_PageUp ? -1 : 1) * (event.modifiers & Qt.ControlModifier ? 12 : 1));
                                    event.accepted = true;
                                }
                            }
                        }
                    }
                }
            }
            RowLayout {
                id: agendaHeading
                objectName: "agendaHeader"
                Layout.fillWidth: true
                Layout.minimumHeight: Theme.controlHeight + Theme.spaceSmall
                Layout.preferredHeight: Layout.minimumHeight
                Text {
                    Layout.fillWidth: true
                    text: root.sameDay(root.selectedDate, root.today) ? "Today" : root.selectedDate.toLocaleDateString(Qt.locale(), "ddd, d MMM")
                    color: Theme.text
                    font.family: Theme.fontFamily
                    font.pixelSize: Theme.fontSize
                    font.weight: Font.DemiBold
                }
            }
            Rectangle {
                id: agendaCard
                objectName: "agendaCard"
                Layout.fillWidth: true
                Layout.fillHeight: !root.inlineMode
                Layout.minimumHeight: root.inlineMode ? implicitHeight : Math.max(Theme.controlHeight * 2, emptyAgenda.visible ? emptyAgenda.implicitHeight + Theme.gap * 2 : 0)
                // The sidebar already scrolls the whole calendar. Let its agenda
                // grow with the events instead of nesting a small scrolling list.
                implicitHeight: root.inlineMode ? Math.max(Theme.calendarAgendaHeight, Math.ceil(agenda.contentHeight) + Theme.gap * 2) : Theme.calendarAgendaHeight
                radius: Theme.menuWidgetRadius
                color: Theme.elevated
                clip: true
                ListView {
                    id: agenda
                    objectName: "calendarAgenda"
                    opacity: root.agendaProgress
                    anchors.fill: parent
                    anchors.margins: Theme.gap
                    clip: true
                    model: root.presentedEvents
                    interactive: !root.inlineMode
                    boundsBehavior: Flickable.StopAtBounds
                    activeFocusOnTab: count > 0
                    keyNavigationEnabled: false
                    function scrollBy(amount) {
                        agendaScroll.to = Math.max(0, Math.min(contentHeight - height, (agendaScroll.running ? agendaScroll.to : contentY) + amount));
                        agendaScroll.restart();
                    }
                    Keys.onDownPressed: event => {
                        if (root.inlineMode)
                            event.accepted = false;
                        else
                            scrollBy(Theme.controlHeight);
                    }
                    Keys.onUpPressed: event => {
                        if (root.inlineMode)
                            event.accepted = false;
                        else
                            scrollBy(-Theme.controlHeight);
                    }
                    Keys.onPressed: event => {
                        if (root.inlineMode)
                            return;
                        if (event.key === Qt.Key_PageDown || event.key === Qt.Key_PageUp) {
                            scrollBy((event.key === Qt.Key_PageDown ? 1 : -1) * height);
                            event.accepted = true;
                        }
                    }
                    onMovementStarted: agendaScroll.stop()
                    NumberAnimation {
                        id: agendaScroll
                        target: agenda
                        property: "contentY"
                        duration: Theme.calendarMotion
                        easing.type: Easing.OutCubic
                    }
                    ScrollBar.vertical: ScrollBar {
                        policy: root.inlineMode ? ScrollBar.AlwaysOff : ScrollBar.AsNeeded
                    }
                    delegate: AbstractButton {
                        id: eventButton
                        required property var modelData
                        required property int index
                        objectName: "calendarEvent-" + index
                        width: agenda.width - (root.inlineMode ? 0 : Theme.gap)
                        implicitHeight: eventContents.implicitHeight + Theme.gap * 2
                        padding: Theme.gap
                        hoverEnabled: true
                        activeFocusOnTab: true
                        Accessible.role: Accessible.Button
                        Accessible.name: modelData.title + ", " + root.eventTime(modelData, root.agendaDate)
                        Accessible.description: "Open event in Calendar"
                        Accessible.onPressAction: clicked()
                        onClicked: root.openEvent(modelData)
                        Keys.onReturnPressed: clicked()
                        Keys.onEnterPressed: clicked()
                        background: ControlCentreButtonSurface {
                            control: eventButton
                        }
                        contentItem: ColumnLayout {
                            id: eventContents
                            spacing: Theme.spaceSmall
                            Text {
                                Layout.fillWidth: true
                                text: eventButton.modelData.title
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSize
                                font.weight: Font.DemiBold
                            }
                            Text {
                                Layout.fillWidth: true
                                text: root.eventTime(eventButton.modelData, root.agendaDate)
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                            }
                        }
                    }
                }
                ColumnLayout {
                    id: emptyAgenda
                    anchors.centerIn: parent
                    width: parent.width - Theme.padding * 2
                    visible: root.presentedEvents.length === 0
                    opacity: root.agendaProgress
                    spacing: Theme.spaceSmall
                    Text {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: eventSource.loading ? "Loading events…" : eventSource.error ? "Events unavailable" : "No events"
                        color: Theme.text
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSize
                    }
                    Text {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        text: eventSource.error || (eventSource.available ? "Nothing scheduled for this day" : "Connect a calendar in Online Accounts")
                        visible: !eventSource.loading
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: Theme.fontSmall
                    }
                }
            }
        }
    }
}
