pragma Singleton
import QtQuick

// Read the compositor policy once per connection. Animation frames stay in
// the compositor; no opacity values travel over the control connection.
QtObject {
    id: root
    property var policies: ({})
    function refresh(namespace = "gnoblin-shell-popup") {
        if (session.capabilities.includes("layer-animation-policy"))
            session.send({
                op: "layer-animation-policy",
                namespace: namespace
            });
    }
    function matches(duration, easing, namespace = "gnoblin-shell-popup") {
        const policy = policies[namespace];
        return !!(session.connected && policy && policy.exit.animation === "fade" && policy.exit.duration === duration && policy.exit.easing === "ease-out-cubic" && easing === Easing.OutCubic);
    }
    function fadesIn(namespace = "gnoblin-shell-popup") {
        return !!(session.connected && policies[namespace]?.enter.animation === "fade");
    }
    property var session: ShortcutSession {
        onCapabilitiesChanged: {
            root.policies = ({});
            for (const namespace of ["gnoblin-shell-popup", "bingux-search", "bingux-switcher", "bingux-bar-tooltip", "gnoblin-dock-tooltip"])
                root.refresh(namespace);
        }
        onLayerAnimationPolicy: function (state) {
            const next = Object.assign({}, root.policies);
            next[state.namespace] = state;
            root.policies = next;
        }
    }
}
