// Pure logic behind BsmReadiness.qml: which surfaces a Bingux process must show,
// when that process counts as launched and ready, and which JSON lines to send
// to the bridge's ready.sock. Tests run this under Node
// (tests/bsm-readiness.test.mjs); nothing here touches QML.
//
// Two Quickshell processes report, each as one `component`:
//   wallpaper  bingux-wallpaper (WallpaperShell.qml): one wallpaper surface per screen
//   desktop    bingux (shell.qml): the top bar, and the dock when the dock is enabled
// A surface counts as rendered only when Qt swapped a frame for its window
// (see BsmSurfaceProbe.qml). Startup.settled is never read here.

var COMPONENTS = ["wallpaper", "desktop"];

function validGeneration(value) {
    return typeof value === "string" && /^[0-9]{1,20}$/.test(value);
}

function validComponent(value) {
    return COMPONENTS.indexOf(value) >= 0;
}

// Roles a component must show on screen before it is ready. `config.dock` is
// the user's dock preference; popouts, tooltips and the other layer surfaces
// are optional and never listed.
function requiredRoles(component, config) {
    if (component === "wallpaper")
        return ["wallpaper"];
    if (component === "desktop")
        return config && config.dock === true ? ["bar", "dock"] : ["bar"];
    return [];
}

// model: {
//   component,
//   config: {loaded: bool, dock: bool},
//   screens: [output name],                       (wallpaper: every one needs a surface)
//   windows: [{role, output, rendered: bool}]     (one per registered window)
// }
// Returns {roles, surfaces: [{role, output, mapped}], launched, ready}.
// `surfaces` lists the required surfaces that exist and know their output.
function evaluate(model) {
    var roles = requiredRoles(model.component, model.config);
    // The wallpaper has no configuration to wait for.
    var loaded = model.component === "wallpaper" || !!(model.config && model.config.loaded);
    var needed = [];
    var windows = model.windows || [];
    function windowsFor(role) {
        return windows.filter(function (w) {
            return w.role === role;
        });
    }
    if (model.component === "wallpaper") {
        (model.screens || []).forEach(function (output) {
            var found = windowsFor("wallpaper").filter(function (w) {
                return w.output === output;
            })[0];
            needed.push({
                role: "wallpaper",
                output: output,
                present: !!found,
                mapped: !!(found && found.rendered)
            });
        });
        if (needed.length === 0)
            needed.push({
                role: "wallpaper",
                output: null,
                present: false,
                mapped: false
            });
    } else {
        roles.forEach(function (role) {
            var found = windowsFor(role);
            if (found.length === 0) {
                needed.push({
                    role: role,
                    output: null,
                    present: false,
                    mapped: false
                });
                return;
            }
            found.forEach(function (w) {
                needed.push({
                    role: role,
                    output: w.output || null,
                    present: !!w.output,
                    mapped: !!w.output && !!w.rendered
                });
            });
        });
    }
    var launched = loaded && roles.length > 0 && needed.every(function (s) {
        return s.present;
    });
    var ready = launched && needed.every(function (s) {
        return s.mapped;
    });
    return {
        roles: roles,
        surfaces: needed.filter(function (s) {
            return s.present && s.output !== null;
        }).map(function (s) {
            return {
                role: s.role,
                output: s.output,
                mapped: s.mapped
            };
        }),
        launched: launched,
        ready: ready
    };
}

function encode(generation, component, event, fields) {
    var object = {
        generation: generation,
        component: component,
        event: event
    };
    for (var key in fields)
        object[key] = fields[key];
    return JSON.stringify(object);
}

// Lines that describe the evaluation. Order: launched, surfaces, ready. Nothing
// is described before `launched`: a surface line always follows it. Once
// launched has been sent on this connection it stays described (unchanged)
// while a window is missing, for example when a monitor appears and its window
// does not exist yet; only `ready` is withheld then.
function desiredLines(generation, component, evaluation, sent) {
    var lines = [];
    var previous = sent && sent.launched;
    if (!evaluation.launched && previous === undefined)
        return lines;
    lines.push({
        key: "launched",
        line: evaluation.launched ? encode(generation, component, "launched", {
            required: evaluation.roles
        }) : previous
    });
    evaluation.surfaces.forEach(function (s) {
        lines.push({
            key: "surface:" + s.output + ":" + s.role,
            line: encode(generation, component, "surface", {
                output: s.output,
                role: s.role,
                mapped: s.mapped
            }),
            surface: s
        });
    });
    if (evaluation.ready)
        lines.push({
            key: "ready",
            line: encode(generation, component, "ready", {})
        });
    return lines;
}

// What to send now. `sent` maps key to the last line written on this
// connection. Returns {lines, sent}: only lines that changed, a withdrawal
// (mapped false) for a surface that is no longer described, and the new map.
// A key that disappears (for example `ready` after a surface went away) is
// forgotten so that it is sent again when it returns.
function diff(generation, component, evaluation, sent) {
    var desired = desiredLines(generation, component, evaluation, sent);
    var next = {};
    var out = [];
    var present = {};
    desired.forEach(function (d) {
        present[d.key] = true;
    });
    Object.keys(sent).forEach(function (key) {
        if (present[key] || key.indexOf("surface:") !== 0)
            return;
        var previous = JSON.parse(sent[key]);
        if (previous.mapped === true)
            out.push(encode(generation, component, "surface", {
                output: previous.output,
                role: previous.role,
                mapped: false
            }));
    });
    desired.forEach(function (d) {
        next[d.key] = d.line;
        if (sent[d.key] !== d.line)
            out.push(d.line);
    });
    return {
        lines: out,
        sent: next
    };
}
