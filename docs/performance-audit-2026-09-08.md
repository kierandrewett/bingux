# Bingux performance audit — 2026-09-08

Scope: source review across the shell UI and its helpers, targeted live measurements, and regression checks. This is not a GPU profile or a long-duration memory soak. Other development sessions and a CPU-heavy archive collector were active; whole-machine CPU changes cannot safely be attributed to these fixes.

## Fixed

### High: microphone watcher generated its own event storm

`microphone-status.py` refreshed on PulseAudio client events. Each refresh ran two `pactl` queries, which themselves created client events. Restricting events to sources, source outputs and server changes breaks that feedback loop. The subscription uses the C locale for stable parsing, and unchanged snapshots no longer cause QML updates.

Live three-second exec traces counted **126 pactl query executions before, zero while idle after reload**. This is a targeted idle measurement, not a claim that real microphone activity needs no queries. Five Python tests pass, covering capture filtering and event facilities.

### Medium: notification history restore was quadratic

`NotificationHistory.js` searched the growing result array for every saved entry. A Set now tracks duplicate keys. A reduction also replaces the argument-spread minimum, avoiding argument-count limits with large existing histories. All valid history remains retained.

A local Node benchmark of 10,000 saved entries measured **422ms before and 19ms after**. These are single-run function timings, not QML frame timings. The regression covers 10,000 unique entries plus a duplicate batch; the history/state suite passes all 23 tests.

### Medium: hidden marquee text could keep animating

`MarqueeText.qml` now includes effective visibility in its animation lifecycle. Hiding it stops and resets the animation; showing it resumes only when active and motion is enabled. An isolated Quickshell QML test verifies movement, hide/reset, continued inactivity, and resumed movement: `tests/marquee-idle.qml`, FAILURES 0.

## Remaining risks, ordered by expected impact

1. **Icon cache growth and broad invalidation.** `OsIcons.qml` retains sources, palettes and requested keys without eviction; resolved images can be data URIs. Each response copies a whole map and invalidates dependent bindings. The Python renderer's 8MiB LRU does not bound these QML maps. Repeated unique icons can therefore increase memory and update cost over a long session. Next step: reference-aware cache ownership plus per-source updates, with a unique-icon memory soak. Blind eviction would blank visible icons because consumers resolve on source changes.
2. **Large notification stacks remain fully materialized.** `NotificationStack.qml` uses a Repeater and repeated scans when reconciling/layouting cards. The history restore fix does not virtualize this UI. Large retained histories can still increase memory and layout time. Next step: virtualize the history viewport while preserving grouping, dismissal animations and stored history; measure 100/1,000/10,000 entries.
3. **Blur fallback can force full redraws.** Gnoblin's `gnoblinBackdropRedraw.js` disables clipped redraws when a mapped blur actor lacks damage-tracking support. This is conditional, not an assertion that the current compositor always redraws everything. Verify the loaded native effect capability, then compare GPU frame cost across monitors with menus and panels animated.
4. **Permanent network polling.** `SystemIndicators.qml` launches its network query every five seconds, even with no network changes. It guards overlapping workers, but still wakes the machine and spawns processes. Prefer NetworkManager change notifications with a slower recovery poll.
5. **Process-panel application matching.** `HardwareDetails.qml` repeatedly matches processes against desktop entries while the panel is visible. Cache executable-to-application identity and invalidate on desktop-entry changes if profiling shows this costs materially on large process trees.

These are source-backed risks; their machine-level cost has not yet been quantified. None warrants deleting user history or silently disabling visible features.

## Coverage and existing safeguards

Reviewed animation/timer lifecycles, process launch/update paths, notification models, dock notifications, audio indicators, media controls, service/device polling, calendar, metrics/process table, Alt+Tab previews, search/document/media previews, icon rendering and compositor redraw integration.

- Media playhead frame updates are gated on visible active playback; reduced motion uses a slower timer.
- Metrics and service-panel polling are visibility-gated; metrics history is bounded.
- Window-switcher preview requests guard pending work and time out.
- Document previews have a 64MiB cache, bounded foreground workers, a separate low-priority warm worker and subprocess timeouts. Page loaders are limited around the viewport.
- Native audio peak monitoring emits changed state rather than unconditional updates each tick.
- Dock notification snapshots avoid rebuilding inactive menu cards on every notification-model update.
- Structural similarity scans found no duplicated JavaScript functions; Python matches were not treated as evidence of a performance defect.

## Live validation limits

After the changes, 12 consecutive `binguxctl status` calls completed with no errors/timeouts: mean **136.7ms**, maximum **143.9ms**. This includes CLI startup/transport overhead and is not an animation latency measurement. The marquee test exercised actual QML in an isolated offscreen shell. This pass did not establish GPU frame pacing, battery impact, or long-session memory stability.


## Second pass

### Fixed: repeated notification reconciliation scans

`NotificationStack.syncEntries()` checked each existing card against every input entry. Its membership predicate also called `cards.get()` for every comparison. Group depth used another linear search per card. Sets and a precomputed depth map now replace these scans.

The production function, executed in Node with a counting model double and 10,000 existing cards in unchanged order, changed from **7,450ms / 50,015,000 model reads** to **22ms / 20,000 reads**. These measurements exclude QML delegate creation and rendering. The regression asserts a linear read budget, correct grouped order, insertion, retirement and restoration. Arbitrary reordering can still require quadratic searches and model moves; full card materialisation remains a separate limitation.

### Fixed: repeated process application scans

`HardwareDetails.applicationFor()` previously scanned every installed desktop entry for each non-exact process lookup. `ProcessApplications.js` builds an alias index from the current desktop entries. The QML property tracks that input so entry changes rebuild the index. Exact desktop IDs retain priority; ambiguous aliases still use the existing fallback. This index has one set of aliases for the installed applications, rather than an accumulating process-name cache.

Tests cover duplicate aliases within one application, ambiguity between applications, exact-match priority, fallback, rebuilding after removal, and 10,000 lookups against 1,000 applications with no further application-property reads after index construction. This proves removal of the repeated scan, not a measured whole-shell CPU reduction.

Validation: six new Node tests passed; the dock notification QML fixture passed; a native Wayland HardwareDetails fixture passed and resolved a real installed application. The first process fixture attempt could not load native PanelWindow under the offscreen backend; rerunning with the native backend passed. The live shell was reloaded and answered its status request. JavaScript similarity analysis found no duplicate functions. The metrics test runner now copies the new JavaScript dependency.
