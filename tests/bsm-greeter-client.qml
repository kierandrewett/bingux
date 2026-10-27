//@ pragma UseQApplication
import QtQuick
import Quickshell
import "../shell/bingux/greeter"
import "../shell/bingux/auth"
import "../shell/bingux/auth/ProgressModel.js" as Progress

// Drives BsmAuthClient and BsmGreeterModel with crafted bsm messages and no
// socket: stale snapshots, prompts and results of another request, progress of
// another launch, malformed lines, image paths outside the cache, account
// changes that must clear secrets, and the sign-in intents. Every message the
// client would send is captured through its `sent` signal.
// Prints BSM_CLIENT_PASSED when every check holds.
ShellRoot {
    id: root
    property int failures: 0
    property var out: []

    BsmGreeterModel {
        id: model
    }
    BsmAuthClient {
        id: client
        model: model
        path: ""
        onSent: message => root.out.push(message)
    }
    property int cleared: 0
    property int rejectedCount: 0
    Connections {
        target: model
        function onSecretsCleared() {
            root.cleared += 1;
        }
        function onRejected() {
            root.rejectedCount += 1;
        }
    }

    function check(name, ok) {
        if (!ok) {
            console.error("BSM_CLIENT_FAILED: " + name);
            failures += 1;
        }
    }

    function user(uid, name, availability, extra) {
        return Object.assign({
            uid: uid,
            user_name: name,
            display_name: name.toUpperCase(),
            avatar_path: null,
            avatar_rev: 0,
            wallpaper_path: null,
            wallpaper_blur_path: null,
            wallpaper_rev: 0,
            show_personal_images: true,
            appearance: {
                color_scheme: "dark",
                accent: "#3584e4",
                clock_format: "24h",
                reduce_motion: false
            },
            availability: availability,
            session: null
        }, extra || {});
    }

    function snapshot(revision, selected, mode, users, progress) {
        return {
            type: "snapshot",
            revision: revision,
            mode: mode,
            selected_uid: selected,
            users: users,
            progress: progress === undefined ? null : progress,
            reduce_motion: false,
            hostname: "bingux"
        };
    }

    function progressModel(generation, revision, reached, extra) {
        return Object.assign({
            type: "progress",
            session: "s0000000b",
            generation: generation,
            revision: revision,
            reached: reached,
            counted_stage: null,
            done: 0,
            total: 0,
            failure: null
        }, extra || {});
    }

    function sentTypes() {
        return out.map(m => m.type);
    }

    function last(type) {
        for (let i = out.length - 1; i >= 0; i--) {
            if (out[i].type === type)
                return out[i];
        }
        return null;
    }

    property real nextChallenge: 200

    // bsm answers the begin_auth in flight with its first prompt.
    function answer() {
        nextChallenge += 1;
        client.handle({ type: "auth_prompt", request_id: last("begin_auth").request_id, challenge_id: nextChallenge, kind: "secret", text: "Password:" });
    }

    function run() {
        const login = {
            mode: "login"
        };
        const users = [user(1000, "alex", "not_signed_in"), user(1001, "sam", "not_signed_in"), user(1002, "robin", "running_locked"), user(1003, "dana", "unavailable")];
        check("the cache directory defaults when its variable is unset", model.cacheDir === "/var/lib/bingux/greeter/");
        model.connected = true;

        // Snapshots: older ones are dropped, newer ones apply.
        client.handle(snapshot(5, 1000, login, users));
        check("first snapshot applies", model.revision === 5 && model.synced && model.selectedUid === 1000);
        client.handle(snapshot(4, 1001, login, users));
        check("an older snapshot is dropped", model.revision === 5 && model.selectedUid === 1000);
        client.handle(snapshot(5, 1001, login, users));
        check("a repeated revision is dropped", model.selectedUid === 1000);
        check("the conversation began for the selected account", last("begin_auth") !== null && last("begin_auth").uid === 1000 && last("begin_auth").intent === "login");

        // Prompts and results of other requests are ignored.
        const request = last("begin_auth").request_id;
        client.handle({ type: "auth_prompt", request_id: request + 7, challenge_id: 31, kind: "secret", text: "Password: " });
        check("a prompt for another request is dropped", model.phase === "waiting" && model.challengeId === 0);
        client.handle({ type: "auth_prompt", request_id: request, challenge_id: 32, kind: "secret", text: "Password: " });
        check("the prompt of our request opens the field", model.promptOpen && model.challengeId === 32 && model.promptPlaceholder === "Password");
        client.handle({ type: "auth_prompt", request_id: null, challenge_id: 99, kind: "visible", text: "Code: " });
        check("a prompt of another challenge is dropped", model.promptKind === "secret");
        client.handle({ type: "auth_result", challenge_id: 99, ok: true, code: null, message: "" });
        check("a result of another challenge is dropped", !model.signingIn);

        // The secret goes out once, is not kept, and a failure restarts the conversation.
        client.respond("hunter2-secret");
        const response = last("auth_response");
        check("the answer is sent with its challenge", response !== null && response.challenge_id === 32 && response.response === "hunter2-secret");
        check("the answer is not kept by the model", JSON.stringify([model.statusText, model.promptText, model.statusLine, model.hint]).indexOf("hunter2") < 0 && model.phase === "verifying");
        client.handle({ type: "auth_result", challenge_id: 32, ok: false, code: "pam_failure", message: "Authentication failed" });
        check("a rejected password shows the error and shakes", model.statusLineIsError && model.statusLine === "Authentication failed" && rejectedCount === 1);
        check("a rejected password starts the next attempt", out.filter(m => m.type === "begin_auth").length === 2);

        // A conversation that fails before any answer does not loop.
        const request2 = last("begin_auth").request_id;
        client.handle({ type: "auth_prompt", request_id: request2, challenge_id: 33, kind: "info", text: "Account locked" });
        check("info text shows without opening the field", !model.promptOpen && model.statusText === "Account locked");
        client.handle({ type: "auth_result", challenge_id: 33, ok: false, code: "pam_failure", message: "Account locked" });
        check("a failure with no answer waits for the user", out.filter(m => m.type === "begin_auth").length === 2 && model.canRetry);
        client.retry();
        check("Try again begins a conversation", out.filter(m => m.type === "begin_auth").length === 3);

        // Changing the account clears secrets and cancels the old challenge.
        const request3 = last("begin_auth").request_id;
        client.handle({ type: "auth_prompt", request_id: request3, challenge_id: 34, kind: "secret", text: "Password:" });
        const clearedBefore = cleared;
        client.handle(snapshot(6, 1001, login, users));
        check("an account change clears the secret fields", cleared > clearedBefore);
        check("an account change cancels the old challenge", last("cancel_auth") !== null && last("cancel_auth").challenge_id === 34);
        check("an account change begins for the new account", last("begin_auth").uid === 1001 && model.phase === "waiting");
        check("a prompt of the old challenge is now dropped", (client.handle({ type: "auth_prompt", request_id: null, challenge_id: 34, kind: "secret", text: "x" }), !model.promptOpen));

        // An unavailable account has no conversation; a running one switches.
        client.handle(snapshot(7, 1003, login, users));
        check("an unavailable account is not authenticated", model.selectedUid === 1003 && !model.canAuthenticate && last("begin_auth").uid === 1001 && model.hint === "This account is unavailable");
        // bsm selects the account of every begin_auth. Two begin_auths in flight
        // fight over the selection, so a snapshot that moves the selection while
        // one is unanswered sends no second one: no ping-pong.
        const inflight = out.filter(m => m.type === "begin_auth").length;
        client.handle(snapshot(8.1, 1000, login, users));
        client.handle(snapshot(8.2, 1001, login, users));
        client.handle(snapshot(8.3, 1000, login, users));
        check("selection snapshots while a begin_auth is unanswered send no more", out.filter(m => m.type === "begin_auth").length === inflight);
        check("no select_user is ever sent for a snapshot", out.filter(m => m.type === "select_user").length === 0);
        answer();
        check("the answer ends the wait", !client.awaitingFirstPrompt);
        client.handle(snapshot(8.4, 1002, login, users));
        check("an account with a running session authenticates as a switch", last("begin_auth").uid === 1002 && last("begin_auth").intent === "switch");

        // Unlock mode: the owner unlocks, anyone else switches.
        const unlock = { mode: "unlock", session: "s0000000a", uid: 1002 };
        answer();
        client.handle(snapshot(9, 1002, unlock, users));
        check("unlock mode: the owner unlocks", model.unlockMode && last("begin_auth").intent === "unlock");
        answer();
        client.handle(snapshot(10, 1001, unlock, users));
        check("unlock mode: another account switches", last("begin_auth").uid === 1001 && last("begin_auth").intent === "switch");
        answer();
        client.handle(snapshot(11, 1002, unlock, users));

        // Images only from the greeter cache.
        const cache = model.cacheDir;
        const photos = [user(1000, "alex", "not_signed_in", { avatar_path: "/home/alex/.face", wallpaper_path: cache + "1000/../../etc/x.png" }), user(1001, "sam", "not_signed_in", { avatar_path: cache + "1001/avatar.png", avatar_rev: 3, wallpaper_path: cache + "1001/w.png", wallpaper_rev: 4 }), user(1002, "robin", "not_signed_in", { avatar_path: cache + "1002/a.png", show_personal_images: false })];
        client.handle(snapshot(12, 1000, login, photos));
        check("an avatar outside the cache is refused", model.avatar.toString() === "");
        check("a path with parent segments is refused", model.wallpaper.toString() === "");
        client.handle(snapshot(13, 1001, login, photos));
        check("cache images load with their revision", model.avatar.toString() === "file://" + cache + "1001/avatar.png?rev=3" && model.wallpaper.toString() === "file://" + cache + "1001/w.png" && model.wallpaperRevision === 4);
        client.handle(snapshot(14, 1002, login, photos));
        check("an opt-out account shows no personal image", model.avatar.toString() === "");

        // Progress: only the launch in flight, newer revisions only.
        client.handle(snapshot(15, 1001, login, users));
        client.handle(progressModel(7, 3, ["auth_accepted"]));
        check("progress with no launch and no sign-in is dropped", model.progress === null);
        client.respond("ignored-no-prompt");
        const request4 = last("begin_auth").request_id;
        client.handle({ type: "auth_prompt", request_id: request4, challenge_id: 40, kind: "secret", text: "Password:" });
        client.respond("pw");
        client.handle({ type: "auth_result", challenge_id: 40, ok: true, code: null, message: "" });
        check("an accepted sign-in shows Signing in and locks the form", model.signingIn && model.statusLine === "Signing in…" && !model.canSubmit);
        client.handle(progressModel(7, 3, ["auth_accepted"]));
        check("progress of the launch that follows our sign-in is adopted", model.progress !== null && Progress.fraction(model.progress) === 5);
        client.handle(progressModel(7, 2, ["auth_accepted", "pam_session"]));
        check("an older revision of the launch is dropped", Progress.fraction(model.progress) === 5);
        client.handle(progressModel(8, 9, ["auth_accepted", "pam_session", "host_connected"]));
        check("another launch is dropped", model.progress.generation === 7 && Progress.fraction(model.progress) === 5);
        client.handle(progressModel(7, 4, ["auth_accepted", "pam_session"]));
        check("a newer revision moves the bar", Progress.fraction(model.progress) === 15);
        client.handle(progressModel(7, 5, ["auth_accepted", "pam_session", "host_connected", "gnoblin_ready"], { counted_stage: null }));
        check("an uncounted stage is indeterminate", Progress.indeterminate(model.progress) && Progress.fraction(model.progress) === 40);
        client.handle(progressModel(7, 6, ["auth_accepted", "pam_session", "host_connected", "gnoblin_ready"], { counted_stage: "outputs_configured", done: 1, total: 2 }));
        check("a counted stage is proportional", !Progress.indeterminate(model.progress) && Progress.fraction(model.progress) === 45 && Progress.counts(model.progress) === "1 of 2" && Progress.label(model.progress) === "Preparing displays");
        check("a malformed progress model is refused", Progress.normalise({ session: "s", reached: ["nope"] }) === null && Progress.normalise(null) === null);

        // A failure stops the bar and can be left.
        client.handle(progressModel(7, 7, ["auth_accepted", "pam_session", "host_connected", "gnoblin_ready"], { failure: { stage: "outputs_configured", code: "timeout", message: "Displays did not respond" } }));
        check("a failure is shown with its message", model.launchFailed && model.showProgress && Progress.failureTitle(model.progress) === "The displays could not be prepared");
        check("a failed launch is not indeterminate", !Progress.indeterminate(model.progress));
        client.backToAccounts();
        check("Back to accounts leaves the failed launch", !model.showProgress);
        check("Back to accounts cancels the failed launch at bsm", last("cancel_operation") !== null);
        client.handle(snapshot(16, 1001, login, users, progressModel(7, 7, ["auth_accepted"], { failure: { stage: "pam_session", code: "pam_failure", message: "x" } })));
        check("a repeated snapshot does not bring a dismissed failure back", !model.showProgress || model.dismissedLaunch !== "");
        client.handle(snapshot(17, 1001, login, users));
        check("a snapshot without progress ends the launch view", model.progress === null);

        // Power requests and refusals.
        client.power("reboot");
        const power = last("power");
        check("a power request carries its action and id", power !== null && power.action === "reboot" && power.request_id > 0);
        client.handle({ type: "operation", operation_id: 501, kind: "power_fade", phase: "preparing", reason: null });
        check("a power operation shows its progress and can be cancelled", model.statusLine === "Restarting…" && model.operationCancellable);
        client.handle({ type: "operation", operation_id: 501, kind: "power_fade", phase: "failed", reason: "inhibited" });
        check("a refused power request shows the reason", model.statusLineIsError && model.statusLine.indexOf("blocking") >= 0);
        client.handle({ type: "error", request_id: 9999, code: "unauthorised", message: "stale" });
        check("an error for another request changes nothing", model.statusLine.indexOf("blocking") >= 0);

        // bsm refuses a begin_auth for an account it no longer has selected.
        client.handle(snapshot(19, 1001, login, users));
        const stale = last("begin_auth");
        const beginsBefore = out.filter(m => m.type === "begin_auth").length;
        client.handle({ type: "error", request_id: stale.request_id, code: "stale_generation", message: "not selected" });
        check("a stale begin_auth is dropped, not resent", !client.awaitingFirstPrompt && model.phase === "idle" && out.filter(m => m.type === "begin_auth").length === beginsBefore);
        client.handle(snapshot(20, 1000, login, users));
        check("the next snapshot's account gets the conversation", last("begin_auth").uid === 1000 && model.selectedUid === 1000);

        // Hostile or broken lines do not throw or change state.
        const revision = model.revision;
        client.parse("not json");
        client.parse("{\"type\":7}");
        client.parse("[]");
        client.parse("");
        client.parse("x".repeat(70000));
        client.parse("{\"type\":\"nonsense\"}");
        check("broken lines are dropped", model.revision === revision);
        client.handle(snapshot(18, "oops", login, "not an array"));
        check("a snapshot of the wrong shape is refused", model.revision === revision);

        // Connection loss drops the conversation and the progress.
        model.disconnected();
        check("losing bsm clears the conversation", !model.synced && model.phase === "idle" && model.progress === null && model.revision === -1);

        // The greeter's palette stays within the whitelist.
        check("AuthTheme keeps a valid accent and refuses a bad one", authTheme.isAccent("#3584e4") && !authTheme.isAccent("red") && !authTheme.isAccent("#12345") && !authTheme.isAccent("#3584e4; evil"));
        authTheme.apply({ color_scheme: "neon", accent: "url(x)", clock_format: "99h", reduce_motion: "yes" });
        check("AuthTheme falls back on unknown values", authTheme.colorScheme === "dark" && authTheme.accentHex === "" && authTheme.clockFormat === "auto" && authTheme.reduceMotionRequested === false);
        authTheme.apply({ color_scheme: "light", accent: "#E66100", clock_format: "12h", reduce_motion: true });
        check("AuthTheme takes a valid record", authTheme.light && authTheme.accentHex === "#e66100" && authTheme.twelveHourClock && authTheme.reduceMotion);

        if (failures === 0)
            console.error("BSM_CLIENT_PASSED");
        // Quickshell connects quit after the configuration has loaded.
        quitLater.start();
    }

    Timer {
        id: quitLater
        interval: 50
        onTriggered: Qt.quit()
    }

    AuthTheme {
        id: authTheme
    }

    Component.onCompleted: run()
}
