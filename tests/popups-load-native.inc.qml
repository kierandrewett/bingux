    // Appended only by the native VM regression runner, never installed.
    readonly property string popupTestGeneration: Date.now() + ":" + Math.random()
    IpcHandler {
        target: "popup-test"
        function status(): string {
            function point(item) {
                const window = topBar.windowFor(item);
                const local = DesktopEditing.point(item, window, item.width / 2, item.height / 2);
                return {
                    x: local.x,
                    y: local.y,
                    visible: item.visible
                };
            }
            function state(item) {
                return item ? {
                    identity: String(item),
                    visible: item.visible,
                    retained: item.retained,
                    closing: item.closing,
                    native: item.nativeWindowsRequested,
                    sameScreen: item.screen === topBar.screen
                } : null;
            }
            return JSON.stringify({
                generation: root.popupTestGeneration,
                clock: point(clockPill),
                metricsButton: point(metricsPill),
                metricsOverflow: topBar.overflows(metricsPill),
                overflowButton: point(overflowButton),
                overflowOpen: barOverflow.visible,
                metricsSettings: metricsLoader.item?.customising ?? null,
                notificationButton: point(notificationButton),
                calendar: state(calendarLoader.item),
                metrics: state(metricsLoader.item),
                notifications: state(notificationCentreLoader.item),
                calendarDate: calendarLoader.item?.selectedDate.getTime() ?? null,
                calendarAnchor: !calendarLoader.item || (calendarLoader.item.anchorWindow === topBar.windowFor(clockPill) && calendarLoader.item.anchorItem === clockPill),
                metricsBindings: !metricsLoader.item || (metricsLoader.item.monitorWidget === metricsPill && metricsLoader.item.anchorWindow === topBar.windowFor(metricsPill) && metricsLoader.item.anchorItem === metricsPill),
                notificationBindings: !notificationCentreLoader.item || notificationCentreLoader.item.notificationSurface === root.notificationSurfaceItem,
                notificationLink: notificationSurface.notificationCentre === notificationCentreLoader.item
            });
        }
        function reloadAfterClose(name: string): string {
            const loaders = {
                calendar: calendarLoader,
                metrics: metricsLoader,
                notifications: notificationCentreLoader
            };
            const item = loaders[name].item;
            item.visible = false;
            const result = JSON.stringify({
                generation: root.popupTestGeneration,
                closing: item.closing
            });
            Qt.callLater(() => Quickshell.reload(false));
            return result;
        }
    }
