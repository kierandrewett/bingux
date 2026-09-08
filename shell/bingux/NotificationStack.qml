import QtQuick
import QtQuick.Controls
import QtQuick.Effects
import Quickshell
import "NotificationHistory.js" as History
import Quickshell.Widgets

// The same cards, grouping, actions and gestures in toasts, history and app menus.
Flickable {
    id: root
    objectName: "notificationViewport"
    signal toastArchived()
    required property var state
    property var presentedEntries: []
    property bool collapseOnDismiss: false
    property bool swipeEnabled: true
    property int activeCollapses: 0
    readonly property real presence: !collapseOnDismiss || cards.count > 1 ? 1
        : cardRepeater.count > 0 && cardRepeater.itemAt(0) ? 1 - cardRepeater.itemAt(0).collapseProgress : 0
    property bool animationsEnabled: true
    readonly property bool reducedMotion: Theme.reducedMotion || !animationsEnabled
    readonly property int cardMotion: animationsEnabled ? Theme.notificationMotion : 0
    readonly property int groupMotion: reducedMotion ? 0 : historyMode ? 180 : Theme.notificationGroupMotion
    property color cardBackground: Theme.surface
    property real cardRadius: Theme.notificationRadius
    property bool cardBorder: true
    property bool rowSeparators: false
    property bool cardShadow: true
    property bool historyMode: false
    property bool groupNotifications: true
    readonly property int visibleStackCards: 4
    property bool active: true
    property real presentationOpacity: 1
    property bool placingHistory: false
    // Settle the cards before their shared viewport slides into view.
    function prepareHistory() {
        placementFinish.stop();
        placingHistory = true;
        for (let i = 0; i < cardRepeater.count; ++i) {
            const card = cardRepeater.itemAt(i);
            if (card && !card.retiring && !card.dismissing) card.settleEntrance();
        }
        historyStart.restart();
    }
    Timer {
        id: historyStart
        interval: 0
        onTriggered: {
            for (let i = 0; i < cardRepeater.count; ++i) {
                const card = cardRepeater.itemAt(i);
                if (card && !card.retiring && !card.dismissing) card.prepareEntrance();
            }
            root.layoutCards();
            placementFinish.start();
        }
    }
    Timer {
        id: placementFinish
        interval: root.reducedMotion ? 0 : Theme.popupOpenMotion + 32
        onTriggered: root.placingHistory = false
    }
    property real trailingInset: Theme.padding
    property real bottomInset: 16
    readonly property real stackHeight: notificationColumn.height + bottomInset
    readonly property int renderedNotificationCount: cards.count
    function resetPresentation() {
        historyStart.stop();
        placementFinish.stop();
        placingHistory = false;
        cards.clear();
        groupBackgrounds.clear();
        syncEntries();
    }

    signal notificationActivated()

    function hasDefaultAction(entry) {
        return state.canActivate(entry);
    }

    function invokeDefaultAction(entry) {
        if (state.activate(entry))
            notificationActivated();
    }

    function hasSecondaryAction(entry) {
        for (let index = 0; index < entry.actions.length; index += 1) {
            if (!entry.actions[index].defaultAction)
                return true;

        }
        return false;
    }

    property var expandedApps: ({})
    property bool updatingGroups: false

    function appKey(entry) {
        if (!groupNotifications) return "notification-" + entry.notification.id;
        return entry.desktopEntry ? entry.desktopEntry.replace(/\.desktop$/, "")
             : entry.appName || "notification-" + entry.notification.id;
    }

    function toggleGroup(key) {
        updatingGroups = true;
        const expanding = !expandedApps[key];
        const expanded = Object.assign({}, expandedApps);
        expanded[key] = expanding;
        expandedApps = expanded;
        // Expansion changes one role, not the contents of every notification.
        for (let index = 0; index < cards.count; index++) {
            if (cards.get(index).groupKey === key) cards.setProperty(index, "groupExpanded", expanding);
        }
        layoutCards();
        updatingGroups = false;
    }

    function dragProgress(card) {
        if (!card || !(card.hasBeenGrabbed || card.dismissing)) return 0;
        const fraction = Math.min(1, Math.max(0, (card.slideOffset + card.dragOffset) / (card.width * 0.35)));
        return 1 - Math.pow(1 - fraction, 3);
    }

    function stackDepth(depth) {
        depth = Math.min(depth, visibleStackCards - 1);
        return depth <= 2 ? depth : 2 + 2 * (1 - Math.pow(0.65, depth - 2));
    }

    // One layout owns the real cards in both states. No substitute stack layers.
    function layoutCards() {
        if (collapseOnDismiss && !groupNotifications) {
            let top = 0;
            let previousRemaining = 0;
            for (let index = 0; index < cardRepeater.count; index++) {
                const card = cardRepeater.itemAt(index);
                if (!card) continue;
                const remaining = 1 - card.collapseProgress;
                if (index > 0) top += notificationColumn.spacing * Math.min(previousRemaining, remaining);
                card.layoutTargetY = top;
                card.layoutInset = 0;
                card.layoutY = top;
                card.layoutHeight = card.naturalHeight * remaining;
                card.layoutReady = true;
                top += card.layoutHeight;
                previousRemaining = remaining;
            }
            notificationColumn.animateHeight = false;
            notificationColumn.height = top;
            return;
        }
        const groups = new Map();
        for (let index = 0; index < cardRepeater.count; index++) {
            const card = cardRepeater.itemAt(index);
            if (!card) continue;
            if (!groups.has(card.groupKey)) groups.set(card.groupKey, []);
            groups.get(card.groupKey).push(card);
        }
        let hasSettledCard = false;
        groups.forEach(group => { if (group.some(card => card.entranceComplete)) hasSettledCard = true; });
        notificationColumn.animateHeight = hasSettledCard && notificationColumn.height > 0;
        let top = 0;
        const retainedGroups = [];
        groups.forEach((group, key) => {
            const head = group.find(card => card.groupHead && !card.retiring) || group[0];
            const expanded = head.groupExpanded && head.groupCount > 1;
            const groupPadding = expanded ? 6 : 0;
            const contentTop = top + groupPadding;
            let cursor = contentTop;
            let bottom = contentTop;
            group.forEach(card => {
                const depth = card.groupDepth;
                card.coveringCard = head;
                card.handoverInProgress = false;
                if (card.coveringSettled) card.awaitingCover = false;
                card.layoutTargetY = expanded ? cursor : top + root.stackDepth(depth) * 8;
                card.layoutInset = expanded ? 6 : root.stackDepth(depth) * 6;
                card.layoutY = expanded ? cursor : top + root.stackDepth(depth) * 8;
                card.layoutHeight = expanded ? card.naturalHeight : head.naturalHeight;
                bottom = Math.max(bottom, (expanded ? cursor : top + root.stackDepth(depth) * 8) + (expanded ? card.naturalHeight : head.naturalHeight));
                if (expanded) cursor += card.naturalHeight + notificationColumn.spacing;
                card.layoutReady = true;
            });
            retainedGroups.push(key);
            const backdrop = {groupKey: key, topY: top, extent: bottom - top + groupPadding, expanded: expanded};
            let existing = -1;
            for (let index = 0; index < groupBackgrounds.count; index++) {
                if (groupBackgrounds.get(index).groupKey === key) { existing = index; break; }
            }
            if (existing < 0) groupBackgrounds.append(backdrop);
            else groupBackgrounds.set(existing, backdrop);
            top = bottom + groupPadding + notificationColumn.spacing;
        });
        for (let index = groupBackgrounds.count - 1; index >= 0; index--) {
            if (retainedGroups.indexOf(groupBackgrounds.get(index).groupKey) < 0) groupBackgrounds.remove(index);
        }
        notificationColumn.height = Math.max(0, top - notificationColumn.spacing);
    }

    Timer { id: layoutTimer; interval: 0; onTriggered: root.layoutCards() }

    // Preserve delegates when another notification arrives or its text changes.
    function syncEntries() {
        if (!cards || !cardRepeater) return;
        const groups = new Map();
        for (const entry of presentedEntries) {
            const key = appKey(entry);
            if (!groups.has(key)) groups.set(key, []);
            groups.get(key).push(entry);
        }
        const entries = [];
        const entryIds = new Set();
        const depths = new Map();
        groups.forEach(group => {
            group.forEach((entry, depth) => {
                entries.push(entry);
                entryIds.add(entry.notification.id);
                depths.set(entry, depth);
            });
        });
        for (let index = cards.count - 1; index >= 0; index -= 1) {
            if (!entryIds.has(cards.get(index).notificationId))
                cards.setProperty(index, "retiring", true);
        }
        let insertionIndex = 0;
        for (let index = 0; index < entries.length; index += 1) {
            if (collapseOnDismiss) {
                while (insertionIndex < cards.count && cards.get(insertionIndex).retiring
                    && !entryIds.has(cards.get(insertionIndex).notificationId)) insertionIndex++;
            }
            const modelIndex = collapseOnDismiss ? insertionIndex++ : index;
            const entry = entries[index];
            const key = appKey(entry);
            const group = groups.get(key);
            const roles = {notificationId: entry.notification.id, entryData: entry, retiring: false,
                groupKey: key, groupCount: group.length, groupDepth: depths.get(entry), groupHead: group[0] === entry,
                groupExpanded: expandedApps[key] === true};
            let existing = -1;
            for (let row = modelIndex; row < cards.count; row += 1) {
                if (cards.get(row).notificationId === entry.notification.id) {
                    existing = row;
                    break;
                }
            }
            if (existing < 0)
                cards.insert(modelIndex, roles);
            else {
                if (existing !== modelIndex)
                    cards.move(existing, modelIndex, 1);
                const delegate = cardRepeater.itemAt(modelIndex);
                const advancing = delegate && roles.groupDepth < delegate.groupDepth;
                const currentX = advancing ? delegate.x : 0;
                const currentY = advancing ? delegate.y : 0;
                const currentHeight = advancing ? delegate.height : 0;
                if (advancing) {
                    delegate.layoutReady = false;
                    delegate.handoverInProgress = true;
                }
                cards.set(modelIndex, roles);
                if (advancing) {
                    // Preserve every backing card at its current rendered position during reindexing.
                    delegate.layoutInset = currentX;
                    delegate.layoutY = currentY;
                    delegate.layoutHeight = currentHeight;
                    delegate.layoutReady = true;
                }
            }
        }
        layoutTimer.restart();
    }

    function removeCard(notificationId) {
        for (let row = cards.count - 1; row >= 0; row -= 1) {
            if (cards.get(row).notificationId === notificationId)
                cards.remove(row);
        }
        layoutTimer.restart();
    }

    ListModel { id: cards; dynamicRoles: true }
    ListModel { id: groupBackgrounds }
    onPresentedEntriesChanged: syncEntries()
    onGroupNotificationsChanged: syncEntries()
    Component.onCompleted: syncEntries()

    Rectangle {
        id: bottomFadeMask
        width: root.width
        height: root.height
        visible: false
        layer.enabled: true
        gradient: Gradient {
            GradientStop { position: 0; color: "white" }
            GradientStop { position: Math.max(0, 1 - 64 / Math.max(1, bottomFadeMask.height)); color: "white" }
            GradientStop { position: 1; color: Qt.rgba(1, 1, 1, 1 - root.bottomFade) }
        }
    }

    contentWidth: width
    contentHeight: notificationColumn.height + bottomInset
    readonly property real bottomFade: Math.min(1, Math.max(0, contentHeight - bottomInset - height - (contentY - originY)) / 64)
    flickableDirection: Flickable.VerticalFlick
    boundsBehavior: Flickable.StopAtBounds
    clip: true
    // Mask the viewport only while there is more content below the fold.
    layer.enabled: bottomFade > 0
    layer.effect: MultiEffect {
        maskEnabled: true
        maskSource: bottomFadeMask
        maskThresholdMin: 0.5
        maskSpreadAtMin: 1.0
    }
    ScrollBar.vertical: ScrollBar {
        policy: ScrollBar.AsNeeded
        onPressedChanged: if (pressed) wheelMotion.stop()
    }
    function clampWheelTarget() {
        const bottom = originY + Math.max(0, contentHeight - height);
        if (wheelMotion.running && (wheelMotion.to > bottom || wheelMotion.to < originY)) {
            wheelMotion.stop();
            contentY = Math.max(originY, Math.min(contentY, bottom));
        }
    }
    onContentHeightChanged: clampWheelTarget()
    onHeightChanged: clampWheelTarget()
    onMovementStarted: wheelMotion.stop()
    NumberAnimation {
        id: wheelMotion
        target: root
        property: "contentY"
        duration: 110
        easing.type: Easing.OutCubic
    }
    WheelHandler {
        enabled: root.active
        target: null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        acceptedModifiers: Qt.KeyboardModifierMask
        onWheel: event => {
            const delta = event.pixelDelta.y || event.angleDelta.y / 120 * 160;
            if (!delta || root.contentHeight <= root.height) { event.accepted = false; return; }
            root.cancelFlick();
            // Accumulate rapid input at its destination, rather than losing
            // wheel distance while the previous step is still animating.
            const destination = wheelMotion.running ? wheelMotion.to : root.contentY;
            const next = root.originY + Math.max(0, Math.min(root.contentHeight - root.height, destination - root.originY - delta));
            wheelMotion.stop();
            if (root.reducedMotion) root.contentY = next;
            else { wheelMotion.to = next; wheelMotion.start(); }
            event.accepted = true;
        }
    }

    Item {
        id: notificationColumn
        width: root.width - root.trailingInset
        readonly property real spacing: root.rowSeparators ? 0 : Theme.gap
        onSpacingChanged: layoutTimer.restart()
        property bool animateHeight: false
        Behavior on height { enabled: !root.placingHistory && notificationColumn.animateHeight; NumberAnimation { duration: root.groupMotion; easing.type: Easing.OutCubic } }

        Repeater {
            model: groupBackgrounds
            delegate: Rectangle {
                // Group-owned, so removing the head card never replaces this surface.
                id: groupSurface
                objectName: "notificationGroupBackground"
                required property string groupKey
                required property real topY
                required property real extent
                required property bool expanded
                property bool ready: false
                Component.onCompleted: ready = true
                readonly property bool showBackground: expanded && root.expandedApps[groupKey] === true
                x: 0
                y: topY
                width: notificationColumn.width
                height: extent
                radius: root.cardRadius + 4
                color: Theme.elevated
                border.width: root.cardBorder ? 1 : 0
                border.color: Theme.outline
                PanelOutline { surface: groupSurface }
                z: -100000
                visible: opacity > 0
                opacity: showBackground ? 0.55 : 0
                // Start collapsed surfaces at zero, without animating from Item's default opacity of one.
                Behavior on opacity { enabled: ready; NumberAnimation { duration: root.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic } }
                Behavior on y { enabled: !root.placingHistory; NumberAnimation { duration: root.groupMotion; easing.type: Easing.OutCubic } }
                Behavior on height { enabled: !root.placingHistory; NumberAnimation { duration: root.groupMotion; easing.type: Easing.OutCubic } }
            }
        }

        Repeater {
            id: cardRepeater
            onItemAdded: layoutTimer.restart()
            onItemRemoved: layoutTimer.restart()
            model: cards

            delegate: Rectangle {
                id: notificationCard
                objectName: "notificationCard"
                readonly property int contentPadding: Theme.notificationPadding

                required property int notificationId
                required property string groupKey
                required property int groupDepth
                required property int groupCount
                required property bool groupHead
                required property bool groupExpanded
                // Do not send every card through the viewport from the same
                // collapsed position. Keep a small margin for smooth scrolling.
                readonly property bool targetNearViewport: layoutTargetY + naturalHeight >= root.contentY - root.originY - 120
                    && layoutTargetY <= root.contentY - root.originY + root.height + 120
                readonly property bool frontNearViewport: groupDepth < root.visibleStackCards
                    && y + height >= root.contentY - root.originY - 120
                    && y <= root.contentY - root.originY + root.height + 120
                readonly property bool nearViewport: !root.historyMode || targetNearViewport || frontNearViewport
                visible: (groupExpanded || groupDepth < root.visibleStackCards) && nearViewport
                    && (!root.updatingGroups || groupDepth < root.visibleStackCards)
                readonly property bool collapsedStack: groupHead && groupCount > 1 && !groupExpanded
                readonly property bool upcomingStackHead: !groupExpanded && groupDepth === 1 && groupCount > 2 && revealProgress > 0
                readonly property real countProgress: groupExpanded || groupHead ? 0 : revealProgress
                readonly property int displayedGroupCount: groupCount - (countProgress >= 0.5 ? 1 : 0)
                property bool layoutReady: false
                property bool entranceComplete: false
                property bool handoverInProgress: false
                property real layoutInset: 0
                property real layoutTargetY: 0
                property real layoutY: 0
                property real layoutHeight: 0
                readonly property real advancingDepth: groupDepth > 0 ? root.stackDepth(groupDepth) - root.stackDepth(groupDepth - 1) : 0
                readonly property real effectiveInset: Math.max(0, layoutInset - advancingDepth * 6 * stackAdvance)
                readonly property bool hasSmallImage: notificationPreview.visible && !notificationPreview.largePreview
                readonly property real naturalHeight: Math.max(cardContents.implicitHeight,
                    hasSmallImage ? Theme.notificationHeaderHeight + Theme.notificationSpacing + 40 : 0) + contentPadding * 2
                onNaturalHeightChanged: layoutTimer.restart()
                clip: true
                layer.enabled: root.cardShadow && visible
                layer.effect: MultiEffect {
                    shadowEnabled: root.cardShadow
                    shadowColor: "#000000"
                    shadowOpacity: 0.22
                    shadowBlur: 0.5
                    blurMax: 12
                    shadowVerticalOffset: 3
                }
                x: effectiveInset
                y: layoutY - advancingDepth * 8 * stackAdvance
                height: layoutHeight + (naturalHeight - layoutHeight) * (groupCount === 2 ? revealProgress : 0)
                Behavior on layoutInset { enabled: !root.placingHistory && notificationCard.nearViewport && notificationCard.layoutReady && notificationCard.entranceComplete; NumberAnimation { duration: root.groupMotion; easing.type: Easing.OutCubic } }
                Behavior on layoutY { enabled: !root.placingHistory && notificationCard.nearViewport && notificationCard.layoutReady && notificationCard.entranceComplete; NumberAnimation { duration: root.groupMotion; easing.type: Easing.OutCubic } }
                Behavior on layoutHeight { enabled: !root.placingHistory && notificationCard.nearViewport && notificationCard.layoutReady && notificationCard.entranceComplete; NumberAnimation { duration: root.groupMotion; easing.type: Easing.OutCubic } }
                Rectangle {
                    objectName: "notificationRowSeparator"
                    visible: root.rowSeparators && (notificationCard.groupHead || notificationCard.groupExpanded)
                        && notificationCard.layoutY + notificationCard.height < notificationColumn.height - 1
                    x: notificationCard.contentPadding
                    y: parent.height - 1
                    width: parent.width - x * 2
                    height: 1
                    color: Theme.outline
                    opacity: 0.45
                }
                required property bool retiring
                enabled: !retiring && (groupHead || groupExpanded)
                onRetiringChanged: {
                    if (retiring) {
                        dismissAnimated();
                    } else if (dismissing) {
                        interruptMotion();
                        snapBack.restart();
                    }
                }
                required property var entryData
                readonly property var entry: entryData
                readonly property var notification: entry.notification
                readonly property bool defaultActionAvailable: root.hasDefaultAction(entry)

                width: notificationColumn.width - effectiveInset * 2
                radius: root.cardRadius
                readonly property color surfaceFill: root.cardBackground
                color: Qt.rgba(surfaceFill.r, surfaceFill.g, surfaceFill.b,
                    groupHead || groupExpanded ? 1 : surfaceFill.a + (1 - surfaceFill.a) * revealProgress)
                border.width: root.cardBorder ? 1 : 0
                border.color: Theme.outline
                PanelOutline { surface: notificationCard }
                Accessible.name: entry.appName + ": " + entry.summary
                Accessible.role: (collapsedStack || defaultActionAvailable) ? Accessible.Button : Accessible.StaticText
                Accessible.focusable: collapsedStack || defaultActionAvailable
                Accessible.onPressAction: collapsedStack ? root.toggleGroup(groupKey) : root.invokeDefaultAction(entry)

                property var coveringCard: null
                property bool awaitingCover: false
                property bool hasBeenGrabbed: false
                readonly property bool revealingFromDrag: !handoverInProgress && !groupExpanded && groupDepth > 0 && coveringCard !== null
                    && (coveringCard.hasBeenGrabbed || coveringCard.dismissing)
                    && coveringCard.slideOffset + coveringCard.dragOffset > 0
                readonly property real stackAdvance: {
                    if (!revealingFromDrag) return 0;
                    return root.dragProgress(coveringCard);
                }
                readonly property real revealProgress: groupDepth === 1 ? stackAdvance : 0
                readonly property bool coveringSettled: coveringCard !== null
                    && coveringCard.slideOffset + coveringCard.dragOffset <= 0
                    && coveringCard.entranceOpacity >= 1
                onGroupHeadChanged: {
                    if (!groupHead && contentsOpacity > 0) awaitingCover = true;
                }
                onCoveringSettledChanged: { if (coveringSettled) awaitingCover = false; }
                property real settledContentsOpacity: groupHead || groupExpanded || awaitingCover ? 1 : 0
                readonly property real contentsOpacity: Math.max(settledContentsOpacity, revealProgress)
                Behavior on settledContentsOpacity {
                    // A revealed card already has visible contents; promotion must not replay their fade-in.
                    enabled: !root.placingHistory && notificationCard.nearViewport && !notificationCard.handoverInProgress
                    NumberAnimation { duration: root.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic }
                }
                property real entranceOpacity: root.historyMode || root.reducedMotion ? 1 : Theme.notificationEntranceOpacity
                readonly property real depthOpacity: Math.max(0.45, 1 - groupDepth * 0.12)
                readonly property real precedingOpacity: Math.max(0.45, 1 - Math.max(0, groupDepth - 1) * 0.12)
                readonly property real stackOpacity: groupExpanded ? 1 : depthOpacity + (precedingOpacity - depthOpacity) * stackAdvance
                opacity: entranceOpacity * stackOpacity * collapseOpacity * root.presentationOpacity
                z: groupHead ? 2 : -groupDepth
                property real slideOffset: root.historyMode || root.reducedMotion ? 0 : Theme.notificationWidth + Theme.padding
                property real dragOffset: 0
                property real dragOrigin: 0
                property real dragDelta: 0
                property real dragActivationDelta: 0
                property bool interruptedMotion: false
                property real collapseProgress: 0
                property real collapseOpacity: 1
                onCollapseProgressChanged: root.layoutCards()
                property bool dismissing: false
                property bool clearAfterDismiss: false
                readonly property bool paused: (root.historyMode && root.active) || cardHover.hovered || notificationMouse.pressed || swipe.active || dismissing
                onPausedChanged: {
                    if (!retiring && notification) root.state.setPaused(notification, paused);
                    expiryProgress.updateProgress();
                }
                onEntryChanged: { if (!retiring && notification && paused) root.state.setPaused(notification, true); }
                transform: Translate { x: notificationCard.slideOffset + notificationCard.dragOffset }
                function settleEntrance() {
                    entranceStart.stop();
                    slideAnimation.stop();
                    slideOffset = 0;
                    finishEntrance();
                }

                function finishEntrance() {
                    entranceComplete = true;
                    entranceFade.stop();
                    entranceOpacity = 1;
                }

                function interruptMotion() {
                    if (collapseDismiss.running) {
                        collapseDismiss.stop();
                        root.activeCollapses--;
                        collapseProgress = 0;
                        collapseOpacity = 1;
                    }
                    hasBeenGrabbed = true;
                    finishEntrance();
                    interruptedMotion = dismissing || slideAnimation.running || snapBack.running;
                    slideAnimation.stop();
                    snapBack.stop();
                    dismissTimer.stop();
                    dragOrigin = slideOffset + dragOffset;
                    dragDelta = 0;
                    slideOffset = 0;
                    dragOffset = dragOrigin;
                    dismissing = false;
                }

                function dismissAnimated() {
                    finishEntrance();
                    if (dismissing)
                        return;
                    clearAfterDismiss = root.historyMode;
                    dismissing = true;
                    if (root.collapseOnDismiss) {
                        root.activeCollapses++;
                        collapseDismiss.start();
                        return;
                    }
                    slideAnimation.to = width + Theme.padding;
                    slideAnimation.restart();
                    dismissTimer.start();
                }

                function prepareEntrance() {
                    if (retiring || dismissing) return;
                    cardContents.forceLayout();
                    root.layoutCards();
                    if (root.historyMode || root.placingHistory) settleEntrance();
                    else {
                        slideAnimation.start();
                        entranceFade.start();
                    }
                }
                Timer { id: entranceStart; interval: 0; onTriggered: notificationCard.prepareEntrance() }
                Component.onCompleted: entranceStart.start()
                NumberAnimation {
                    id: entranceFade
                    target: notificationCard
                    property: "entranceOpacity"
                    to: 1
                    duration: root.cardMotion
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    id: slideAnimation
                    onFinished: { if (!notificationCard.dismissing) notificationCard.entranceComplete = true; }
                    target: notificationCard
                    property: "slideOffset"
                    to: 0
                    duration: root.cardMotion
                    easing.type: Easing.OutCubic
                }
                NumberAnimation {
                    id: snapBack
                    target: notificationCard
                    property: "dragOffset"
                    to: 0
                    duration: root.cardMotion
                    easing.type: Easing.OutCubic
                }
                ParallelAnimation {
                    id: collapseDismiss
                    NumberAnimation { target: notificationCard; property: "collapseOpacity"; to: 0; duration: Theme.reducedMotion ? 0 : 140; easing.type: Easing.OutCubic }
                    SequentialAnimation {
                        PauseAnimation { duration: Theme.reducedMotion ? 0 : 40 }
                        NumberAnimation { target: notificationCard; property: "collapseProgress"; to: 1; duration: Theme.reducedMotion ? 0 : 280; easing.type: Easing.InOutCubic }
                    }
                    onFinished: {
                        root.activeCollapses--;
                        notificationCard.completeDismissal();
                    }
                }
                function completeDismissal() {
                    if (!retiring && notification) {
                        if (clearAfterDismiss) root.state.dismiss(notification);
                        else root.state.archive(notification);
                    }
                    if (!root.historyMode && notification && root.state.allEntries.some(entry => entry.notification === notification)) root.toastArchived();
                    root.removeCard(notificationId);
                }
                Timer {
                    id: dismissTimer
                    interval: root.cardMotion
                    onTriggered: notificationCard.completeDismissal()
                }

                HoverHandler { id: cardHover }
                DragHandler {
                    id: swipe
                    enabled: root.swipeEnabled
                    target: null
                    acceptedButtons: Qt.LeftButton
                    dragThreshold: Theme.spaceSmall
                    yAxis.enabled: false
                    onActiveTranslationChanged: {
                        if (active) {
                            notificationCard.dragDelta = activeTranslation.x;
                            // Start from the activation point instead of jumping through the dead zone.
                            const distance = activeTranslation.x - notificationCard.dragActivationDelta;
                            notificationCard.dragOffset = Math.max(0, notificationCard.dragOrigin + distance);
                        }
                    }
                    onActiveChanged: {
                        if (active) {
                            notificationCard.dragActivationDelta = centroid.scenePosition.x - centroid.scenePressPosition.x;
                            snapBack.stop();
                        } else {
                            const pulledBack = notificationCard.dragOrigin > 0 && notificationCard.dragDelta < 0;
                            if (!pulledBack && notificationCard.dragOffset >= Math.min(Theme.notificationDismissDistance, notificationCard.width * 0.15))
                                notificationCard.dismissAnimated();
                            else
                                snapBack.restart();
                        }
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    radius: notificationCard.radius
                    color: Theme.pressed
                    opacity: !(notificationCard.defaultActionAvailable || notificationCard.collapsedStack) ? 0
                        : notificationMouse.pressed ? 1 : cardHover.hovered ? .4 : 0
                }

                MouseArea {
                    id: notificationMouse
                    anchors.fill: parent
                    cursorShape: Qt.ArrowCursor
                    onPressed: if (root.swipeEnabled) notificationCard.interruptMotion()
                    onReleased: {
                        if (!swipe.active && !notificationCard.dismissing)
                            snapBack.restart();
                    }
                    onCanceled: {
                        if (!swipe.active && !notificationCard.dismissing)
                            snapBack.restart();
                    }
                    onClicked: {
                        if (!notificationCard.dismissing && !notificationCard.interruptedMotion) {
                            if (notificationCard.collapsedStack) root.toggleGroup(notificationCard.groupKey);
                            else if (notificationCard.defaultActionAvailable) root.invokeDefaultAction(notificationCard.entry);
                        }
                    }
                }

                OsIconImage {
                    id: applicationIcon
                    opacity: notificationCard.contentsOpacity

                    width: Theme.notificationIconSize
                    height: Theme.notificationIconSize
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.topMargin: notificationCard.contentPadding
                    anchors.leftMargin: notificationCard.contentPadding
                    source: {
                        const icon = notificationCard.entry.appIcon;
                        if (icon.startsWith("/") || icon.startsWith("file:") || icon.startsWith("image:"))
                            return icon;
                        return Quickshell.iconPath(icon, "application-x-executable");
                    }
                }

                Column {
                    id: cardContents
                    opacity: notificationCard.contentsOpacity

                    width: parent.width - notificationCard.contentPadding * 2 - Theme.notificationIconSize - Theme.padding
                    anchors.top: parent.top
                    anchors.topMargin: notificationCard.contentPadding
                    anchors.right: parent.right
                    anchors.rightMargin: notificationCard.contentPadding
                    spacing: Theme.notificationSpacing

                    Item {
                        width: parent.width
                        height: Theme.notificationHeaderHeight

                        Text {
                            anchors.left: parent.left
                            anchors.right: expiryProgress.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            color: Theme.text
                            font.weight: Font.Medium
                            elide: Text.ElideRight
                            font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                            text: notificationCard.entry.appName || notificationCard.entry.desktopEntry || "Notification"
                            textFormat: Text.PlainText
                        }

                        Canvas {
                            id: expiryProgress
                            objectName: "notificationExpiryProgress"
                            visible: !root.historyMode && notificationCard.entry.timeoutMs > 0
                            width: visible ? Theme.iconSize : 0
                            height: Theme.iconSize
                            anchors.right: receivedTime.left
                            anchors.rightMargin: Theme.gap
                            anchors.verticalCenter: parent.verticalCenter
                            property real progress: 0
                            Accessible.role: Accessible.ProgressBar
                            Accessible.name: "Notification timeout"
                            Accessible.description: Math.round(progress * 100) + "% elapsed"
                            function updateProgress() {
                                progress = notificationCard.retiring ? 1 : root.state.expiryProgress(notificationCard.notification);
                            }
                            onProgressChanged: requestPaint()
                            Component.onCompleted: updateProgress()
                            onPaint: {
                                const context = getContext("2d");
                                context.reset();
                                const radius = Math.max(0, width / 2 - 2);
                                if (radius <= 0) return;
                                context.lineWidth = 2;
                                context.strokeStyle = Theme.outline;
                                context.beginPath();
                                context.arc(width / 2, height / 2, radius, 0, Math.PI * 2);
                                context.stroke();
                                if (progress > 0) {
                                    context.strokeStyle = Theme.accent;
                                    context.lineCap = "round";
                                    context.beginPath();
                                    context.arc(width / 2, height / 2, radius, -Math.PI / 2, -Math.PI / 2 + progress * Math.PI * 2);
                                    context.stroke();
                                }
                            }
                            Timer {
                                interval: 40
                                repeat: true
                                running: expiryProgress.visible && !notificationCard.paused && !notificationCard.retiring
                                    && notificationCard.y + notificationCard.height >= root.contentY
                                    && notificationCard.y <= root.contentY + root.height
                                onTriggered: expiryProgress.updateProgress()
                            }
                        }

                        Text {
                            id: receivedTime
                            objectName: "notificationReceivedTime"
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(implicitWidth, Theme.controlHeight)
                            horizontalAlignment: Text.AlignRight
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: Theme.fontSmall
                            text: Qt.formatDateTime(new Date(notificationCard.entry.receivedAt), "HH:mm")
                            opacity: cardHover.hovered ? 0 : 1
                            Accessible.name: "Received at " + Qt.formatDateTime(new Date(notificationCard.entry.receivedAt), "ddd d MMM, HH:mm")
                        }

                        Rectangle {
                            id: closeButton
                            objectName: "notificationCloseButton"
                            opacity: cardHover.hovered ? 1 : 0
                            enabled: cardHover.hovered
                            Accessible.ignored: !enabled
                            radius: Theme.radius
                            color: closeMouse.pressed ? Theme.pressed : closeMouse.containsMouse ? Theme.hover : "transparent"

                            width: Theme.controlHeight
                            height: Theme.controlHeight
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            Accessible.name: "Dismiss notification"
                            Accessible.role: Accessible.Button
                            Accessible.focusable: true
                            Accessible.onPressAction: notificationCard.dismissAnimated()

                            SymbolicIcon {
                                anchors.centerIn: parent
                                implicitSize: 16
                                source: Quickshell.iconPath("window-close-symbolic", "edit-clear-symbolic")
                            }

                            MouseArea {
                                id: closeMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.ArrowCursor
                                onClicked: notificationCard.dismissAnimated()
                            }

                        }

                    }

                    Text {
                        width: parent.width - (notificationCard.hasSmallImage ? 48 : 0)
                        color: Theme.text
                        elide: Text.ElideRight
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                        font.weight: Font.DemiBold
                        maximumLineCount: 2
                        text: notificationCard.entry.summary || "Notification"
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }

                    Text {
                        width: parent.width - (notificationCard.hasSmallImage ? 48 : 0)
                        visible: notificationCard.entry.body.length > 0
                        color: Theme.muted
                        font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize
                        lineHeight: 1.2
                        elide: Text.ElideRight
                        maximumLineCount: notificationCard.groupExpanded ? 30 : 3
                        text: notificationCard.entry.body
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }

                    Image {
                        id: notificationPreview
                        objectName: "notificationImagePreview"
                        onStatusChanged: if (status === Image.Ready) cachePreview.restart()
                        onOpacityChanged: if (opacity === 1 && status === Image.Ready) cachePreview.restart()
                        Timer {
                            id: cachePreview
                            interval: 150
                            onTriggered: if (typeof root.state.cacheImage === "function") root.state.cacheImage(notificationCard.entry, notificationPreview)
                        }
                        readonly property bool largePreview: History.largeImage(notificationCard.entry)
                        parent: largePreview ? cardContents : notificationCard
                        x: largePreview ? 0 : notificationCard.width - notificationCard.contentPadding - width
                        y: largePreview ? 0 : notificationCard.contentPadding + Theme.notificationHeaderHeight + Theme.notificationSpacing
                        width: largePreview ? cardContents.width : 40
                        opacity: notificationCard.contentsOpacity
                        height: !visible ? 0 : largePreview && implicitWidth > 0 ? Math.min(width * implicitHeight / implicitWidth, 180) : 40
                        visible: source.toString() !== "" && status !== Image.Error
                        readonly property string imageSource: notificationCard.entry.image || ""
                        readonly property string normalizedImage: !imageSource || imageSource.startsWith("/") || /^[a-z][a-z0-9+.-]*:/i.test(imageSource)
                            ? imageSource : Quickshell.iconPath(imageSource)
                        readonly property bool nativePixels: normalizedImage.startsWith("data:") || (normalizedImage.startsWith("image://") && !normalizedImage.startsWith("image://icon/"))
                        onNormalizedImageChanged: if (!nativePixels) OsIcons.resolve(normalizedImage)
                        Component.onCompleted: if (!nativePixels) OsIcons.resolve(normalizedImage)
                        source: nativePixels ? normalizedImage : OsIcons.sources[normalizedImage] || ""
                        // Decode for the available column width, not the card's
                        // animated inset. Changing sourceSize reloads the image.
                        sourceSize.width: largePreview ? Math.round(Math.max(0, notificationColumn.width - notificationCard.contentPadding * 2) * 2) : 80
                        sourceSize.height: 360
                        asynchronous: true
                        retainWhileLoading: true
                        fillMode: Image.PreserveAspectFit
                        Accessible.role: Accessible.Graphic
                        Accessible.name: "Notification image preview"
                    }

                    Item {
                        visible: notificationCard.hasSmallImage
                        width: parent.width
                        height: visible ? Math.max(0, notificationPreview.y + notificationPreview.height - cardContents.y - y) : 0
                    }

                    AbstractButton {
                        id: groupToggle
                        objectName: "notificationGroupToggle"
                        visible: (notificationCard.groupHead && notificationCard.groupCount > 1) || notificationCard.upcomingStackHead
                        width: Math.min(parent.width, groupLabel.implicitWidth + groupCounter.width + 46)
                        height: 28
                        text: notificationCard.displayedGroupCount + " notifications · " + (notificationCard.groupExpanded ? "Show less" : "Show more")
                        Accessible.role: Accessible.Button
                        Accessible.name: notificationCard.groupExpanded ? "Collapse notifications" : "Show all " + notificationCard.displayedGroupCount + " notifications"
                        onClicked: root.toggleGroup(notificationCard.groupKey)
                        background: Rectangle {
                            radius: height / 2
                            color: groupToggle.down ? Theme.pressed : groupToggle.hovered ? Theme.hover : Theme.elevated
                            border.width: groupToggle.activeFocus ? 1 : 0
                            border.color: Theme.accent
                        }
                        contentItem: Item {
                            Item {
                                id: groupCounter
                                objectName: "notificationGroupCounter"
                                anchors.left: parent.left
                                anchors.leftMargin: 11
                                anchors.verticalCenter: parent.verticalCenter
                                width: Math.max(currentCount.implicitWidth, nextCount.implicitWidth)
                                height: currentCount.implicitHeight + 2
                                clip: true
                                Behavior on width { NumberAnimation { duration: root.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic } }
                                Text {
                                    id: currentCount
                                    text: notificationCard.groupCount
                                    y: 1 - notificationCard.countProgress * 8
                                    opacity: 1 - notificationCard.countProgress
                                    color: groupLabel.color
                                    font: groupLabel.font
                                }
                                Text {
                                    id: nextCount
                                    text: Math.max(1, notificationCard.groupCount - 1)
                                    y: 1 + (1 - notificationCard.countProgress) * 8
                                    opacity: notificationCard.countProgress
                                    color: groupLabel.color
                                    font: groupLabel.font
                                }
                            }
                            Text {
                                id: groupLabel
                                anchors.left: groupCounter.right
                                anchors.right: groupChevron.left
                                anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                text: " notifications · " + (notificationCard.groupExpanded ? "Show less" : "Show more")
                                elide: Text.ElideRight
                                color: groupToggle.hovered || groupToggle.down ? Theme.text : Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: Theme.fontSmall
                                font.weight: Font.Medium
                            }
                            SymbolicIcon {
                                id: groupChevron
                                anchors.right: parent.right
                                anchors.rightMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                implicitSize: 12
                                source: Quickshell.iconPath("pan-down-symbolic", "go-down-symbolic")
                                color: groupLabel.color
                                rotation: notificationCard.groupExpanded ? 180 : 0
                                Behavior on rotation { NumberAnimation { duration: root.reducedMotion ? 0 : 180; easing.type: Easing.OutCubic } }
                            }
                        }
                    }

                    Flow {
                        objectName: "notificationActions"
                        width: parent.width
                        height: visible ? implicitHeight : 0
                        visible: root.hasSecondaryAction(notificationCard.entry)
                        spacing: Theme.gap

                        Repeater {
                            model: notificationCard.entry.actions

                            delegate: ActionButton {
                                id: actionButton

                                required property var modelData
                                objectName: "notificationAction_" + (modelData.action ? modelData.action.identifier : "")
                                wrapLabel: true
                                enabled: !!modelData.action && typeof modelData.action.invoke === "function"
                                width: visible ? Math.min(parent.width, implicitWidth) : 0
                                height: visible ? implicitHeight : 0
                                text: modelData.text
                                horizontalPadding: Theme.gap
                                iconName: ({copy: "edit-copy-symbolic", save: "document-save-as-symbolic", discard: "user-trash-symbolic"})[modelData.action.identifier] || ""
                                visible: !modelData.defaultAction
                                onClicked: modelData.action.invoke()
                                Accessible.onPressAction: modelData.action.invoke()

                            }

                        }

                    }

                }

            }

        }

    }

}
