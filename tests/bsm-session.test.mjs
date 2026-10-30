import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

// BsmSession.js is the pure half of the shell's bsm client. These tests pin the
// rule that matters: an accepted request is never reported as a success; only
// bsm's monitor events finish an operation.
const source = readFileSync(new URL("../shell/bingux/BsmSession.js", import.meta.url), "utf8");
const S = {};
vm.runInNewContext(source, S, { filename: "BsmSession.js" });

const transition = (id, phase, reason = null, kind = "lock") =>
    S.parseMonitorEvent(JSON.stringify({ event: "transition", operation_id: id, kind, phase, reason }));
const finished = (id, success, reason = null) =>
    S.parseMonitorEvent(JSON.stringify({ event: "operation_finished", operation_id: id, success, reason }));
const accepted = (id) => ({ ok: true, id: String(id) });
// Objects made inside the vm context have another prototype; compare as plain data.
const plain = (value) => JSON.parse(JSON.stringify(value));

test("integrated mode follows BSM_SESSION_ID and nothing else", () => {
    assert.equal(S.integrated(undefined), false);
    assert.equal(S.integrated(null), false);
    assert.equal(S.integrated(""), false);
    assert.equal(S.integrated("s0000000a"), true);
});

test("only the six documented actions become a bsmctl command", () => {
    for (const action of ["lock", "logout", "switch-user", "suspend", "reboot", "poweroff"])
        assert.deepEqual(plain(S.requestCommand(action)), ["bsmctl", action, "--json"]);
    for (const bad of ["", "shutdown", "lock; rm -rf ~", "monitor", "--help", null, undefined])
        assert.equal(S.requestCommand(bad), null, String(bad));
    assert.deepEqual(plain(S.monitorCommand()), ["bsmctl", "monitor", "--json"]);
});

test("a request answer needs an operation id; exit status 0 alone is not enough", () => {
    assert.deepEqual(plain(S.parseRequest(0, '{"operation_id": 7}\n', "")), { ok: true, id: "7" });
    assert.deepEqual(plain(S.parseRequest(0, '{"operation_id": "18446744073709551615"}', "")), {
        ok: true,
        id: "18446744073709551615",
    });
    for (const out of ["", "ok\n", "{}", '{"operation_id": -1}', '{"operation_id": 1.5}', '{"operation_id": "x"}'])
        assert.equal(S.parseRequest(0, out, "").code, "no_operation", out);
});

test("a refused request reports bsm's reason code", () => {
    assert.deepEqual(plain(S.parseRequest(1, '{"error":"inhibited","message":"A game is running"}', "")), {
        ok: false,
        code: "inhibited",
        detail: "A game is running",
    });
    assert.equal(S.parseRequest(2, '{"error":{"code":"busy","message":"x"}}', "").code, "busy");
    // bsmctl missing or bsm not running: no JSON, only stderr.
    const none = S.parseRequest(127, "", "bsmctl: cannot connect to org.bingux.Session1\nmore");
    assert.equal(none.code, "unavailable");
    assert.equal(none.detail, "bsmctl: cannot connect to org.bingux.Session1");
});

test("monitor lines: unknown, malformed and invalid events are ignored", () => {
    for (const line of ["", "nope", "[]", "null", '{"event":"mystery"}', '{"event":"transition","operation_id":1,"phase":"sideways"}', '{"event":"transition","phase":"failed"}', '{"event":"operation_finished","operation_id":1,"success":"yes"}'])
        assert.equal(S.parseMonitorEvent(line), null, line);
    assert.equal(S.parseMonitorEvent('{"event":"sessions_changed","revision":3}').event, "sessions_changed");
});

test("an accepted request is pending, never completed", () => {
    let t = S.begin(S.newTracker(), "reboot");
    assert.equal(t.current.status, "requesting");
    t = S.answered(t, accepted(5));
    assert.equal(t.current.status, "pending");
    assert.equal(S.isActive(t), true);
    for (const phase of ["requested", "authorising", "preparing", "animating", "committed"]) {
        t = S.observe(t, transition(5, phase, null, "power_fade"));
        assert.equal(t.current.status, "pending", phase);
        assert.equal(t.current.phase, phase);
    }
    t = S.observe(t, transition(5, "completed", null, "power_fade"));
    assert.equal(t.current.status, "completed");
    assert.equal(S.isActive(t), false);
});

test("Failed carries bsm's reason code into the message", () => {
    let t = S.answered(S.begin(S.newTracker(), "poweroff"), accepted(9));
    t = S.observe(t, transition(9, "preparing", null, "power_fade"));
    t = S.observe(t, transition(9, "failed", "inhibited", "power_fade"));
    assert.equal(t.current.status, "failed");
    assert.equal(t.current.code, "inhibited");
    const text = S.describeFailure(t.current.code, t.current.detail);
    assert.match(text, /blocking/);
    assert.match(text, /\(inhibited\)$/);
});

test("Cancelled is its own outcome and is not an error", () => {
    let t = S.answered(S.begin(S.newTracker(), "logout"), accepted(2));
    t = S.observe(t, transition(2, "cancelled", "transition_interrupted", "logout_fade"));
    assert.equal(t.current.status, "cancelled");
    assert.equal(t.current.code, "transition_interrupted");
    t = S.answered(S.begin(S.newTracker(), "logout"), accepted(3));
    t = S.observe(t, finished(3, false, "cancelled"));
    assert.equal(t.current.status, "cancelled");
});

test("operation_finished completes or fails the operation", () => {
    // The tracker remembers operations by id, so each case starts from its own.
    const open = (id) => S.answered(S.begin(S.newTracker(), "suspend"), accepted(id));
    assert.equal(S.observe(open(4), finished(4, true)).current.status, "completed");
    const failed = S.observe(open(5), finished(5, false, "logind_failure"));
    assert.equal(failed.current.status, "failed");
    assert.equal(failed.current.code, "logind_failure");
    assert.equal(S.observe(open(6), finished(6, false)).current.code, "internal");
});

test("events that arrive before the request answer are not lost", () => {
    let t = S.begin(S.newTracker(), "lock");
    t = S.observe(t, transition(11, "requested"));
    t = S.observe(t, transition(11, "animating"));
    t = S.observe(t, transition(11, "failed", "already_locked"));
    assert.equal(t.current.status, "requesting", "unrelated until the id is known");
    t = S.answered(t, accepted(11));
    assert.equal(t.current.status, "failed");
    assert.equal(t.current.code, "already_locked");
});

test("events for another operation, stale or repeated events change nothing", () => {
    let t = S.answered(S.begin(S.newTracker(), "reboot"), accepted(20));
    const other = S.observe(t, transition(21, "failed", "busy"));
    assert.equal(other.current.status, "pending");
    t = S.observe(t, transition(20, "completed"));
    assert.equal(t.current.status, "completed");
    // A late failure for the same id must not rewrite the finished outcome.
    const late = S.observe(t, transition(20, "failed", "internal"));
    assert.equal(late.current.status, "completed");
    assert.equal(S.observe(t, finished(20, false, "internal")).current.status, "completed");
    // Phases never go backwards after a final event.
    assert.equal(S.observe(t, transition(20, "requested")).current.phase, "completed");
});

test("a refused request fails at once with the reason", () => {
    let t = S.begin(S.newTracker(), "lock");
    t = S.answered(t, S.parseRequest(1, '{"error":"unauthorised"}', ""));
    assert.equal(t.current.status, "failed");
    assert.equal(t.current.code, "unauthorised");
});

test("losing the monitor fails an open operation instead of waiting forever", () => {
    let t = S.answered(S.begin(S.newTracker(), "reboot"), accepted(30));
    t = S.monitorLost(t);
    assert.equal(t.current.status, "failed");
    assert.equal(t.current.code, "monitor_lost");
    // With nothing open there is nothing to fail.
    assert.equal(S.monitorLost(S.newTracker()).current, null);
    const done = S.observe(S.answered(S.begin(S.newTracker(), "lock"), accepted(31)), transition(31, "completed"));
    assert.equal(S.monitorLost(done).current.status, "completed");
});

test("only one operation is open at a time; a finished one can be dismissed", () => {
    let t = S.begin(S.newTracker(), "reboot");
    assert.equal(S.begin(t, "poweroff").current.action, "reboot");
    t = S.answered(t, accepted(1));
    assert.equal(S.begin(t, "poweroff").current.action, "reboot");
    assert.equal(S.clearFinished(t).current.status, "pending", "an open operation is not dismissed");
    t = S.observe(t, transition(1, "completed"));
    assert.equal(S.clearFinished(t).current, null);
    assert.equal(S.begin(S.clearFinished(t), "poweroff").current.action, "poweroff");
});

test("remembered operations are bounded", () => {
    let t = S.newTracker();
    for (let id = 0; id < 200; id++) t = S.observe(t, transition(id, "requested"));
    assert.ok(Object.keys(t.seen).length <= 32);
    assert.equal(t.seen["0"], undefined);
    assert.ok(t.seen["199"]);
});

test("every proto error code has a sentence and the sentence is not the code", () => {
    const codes = [...readFileSync(new URL("../packages/bsm-proto/src/error.rs", import.meta.url), "utf8").matchAll(/Self::\w+ => "([a-z_]+)"/g)].map((m) => m[1]);
    assert.ok(codes.length >= 18, "parsed the proto error codes");
    for (const code of codes) assert.notEqual(S.messageFor(code), S.messageFor("not-a-code"), code);
    for (const extra of ["unavailable", "monitor_lost", "no_operation"]) assert.ok(S.messageFor(extra));
});
