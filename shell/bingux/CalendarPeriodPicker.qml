import QtQuick
import QtQuick.Layouts

GridLayout {
    id: root
    required property int level
    required property int year
    required property date selectedDate
    signal chosen(int year, int month)
    signal pageRequested(int direction)
    signal backRequested()
    columns: 3
    rowSpacing: 6
    columnSpacing: 6
    readonly property int rangeStart: level === 2 ? Math.floor(year / 10) * 10 : Math.floor(year / 100) * 100
    function focusCell(index) {
        const cell = cells.itemAt(Math.max(0, Math.min(11, index)));
        if (cell) cell.forceActiveFocus(Qt.OtherFocusReason);
    }
    function focusSelection() {
        focusCell(level === 1 ? selectedDate.getMonth() : level === 2 ? year - rangeStart + 1 : Math.floor((year - rangeStart) / 10) + 1);
    }
    Repeater {
        id: cells
        model: 12
        ActionButton {
            id: cell
            required property int index
            readonly property int cellYear: root.level === 1 ? root.year : root.rangeStart + (index - 1) * (root.level === 2 ? 1 : 10)
            readonly property bool current: root.level === 1 ? root.selectedDate.getFullYear() === cellYear && root.selectedDate.getMonth() === index
                : root.level === 2 ? root.selectedDate.getFullYear() === cellYear
                : root.selectedDate.getFullYear() >= cellYear && root.selectedDate.getFullYear() <= cellYear + 9
            objectName: "calendarPeriod-" + index
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 0
            flat: true
            hoverEnabled: true
            horizontalPadding: 4
            enabled: cellYear >= 1 && cellYear <= 9999 && (root.level < 3 || cellYear + 9 <= 9999)
            text: root.level === 1 ? new Date(2000, index, 1).toLocaleDateString(Qt.locale(), "MMM")
                : root.level === 2 ? String(cellYear) : cellYear + "–" + (cellYear + 9)
            Accessible.description: root.level === 1 ? "Show days in " + text + " " + cellYear
                : root.level === 2 ? "Show months in " + text : "Show years from " + text
            background: ControlCentreButtonSurface { control: cell; selected: cell.current }
            onClicked: root.chosen(cellYear, root.level === 1 ? index : 0)
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Left) root.focusCell(index - 1);
                else if (event.key === Qt.Key_Right) root.focusCell(index + 1);
                else if (event.key === Qt.Key_Up) root.focusCell(index - 3);
                else if (event.key === Qt.Key_Down) root.focusCell(index + 3);
                else if (event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown) root.pageRequested(event.key === Qt.Key_PageUp ? -1 : 1);
                else if (event.key === Qt.Key_Escape) root.backRequested();
                else return;
                event.accepted = true;
            }
        }
    }
}
