# Calendar popout

The clock shows local time, including seconds, with tabular digits and matching
date/time weights. The calendar uses the control centre's shared surfaces and
buttons. Click a date for its agenda; arrows move by day/week, Page Up/Down by
month, and Home returns to today. Only today has a blue fill; selection uses a
neutral highlight and outline only after clicking a date. Month navigation and
hover do not fill a day; keyboard navigation uses a focus outline. Dots mark days containing events. Long agendas scroll inside a stable
height card, and the Today action always reserves its space. Month changes slide
the outgoing and incoming pages together inside a clipped viewport, while weekday
headings stay still. Reversing retraces the transition; rapid forward requests
coalesce to the latest requested month. Agendas fade out before replacing their
contents and resetting scroll, then fade in without any vertical movement.
Reduced-motion mode removes those transitions. Scroll over the month heading
to change months; Ctrl+Page Up/Down in the grid changes years. Tab to the agenda
and use arrows/Page Up/Down for smooth keyboard scrolling. Event titles can use
two lines before truncation.

## Event source

`CalendarEvents.qml` starts `calendar-events.py` only while the popout is open.
It reads `org.gnome.Shell.CalendarServer` over the session bus. The service uses
Evolution Data Server calendars, including supported Online Accounts sources.
No credentials are read, events modified, or new account sync implemented.
The existing calendar service handles recurrence expansion and timezones.
The protocol is documented in the [upstream calendar server source](https://github.com/GNOME/gnome-shell/blob/main/src/calendar-server/gnome-shell-calendar-server.c).

Requests cover the displayed month plus the neighbouring grid days. Signal
updates, removals and disappearing sources update the agenda. Events use an
exclusive end time; events covering the entire selected local day display as
"All day". This service does not expose an explicit all-day flag.

The packaged helper has Python/Gio dependencies bundled; local development uses
`python3` with PyGObject. `BINGUX_CALENDAR_HELPER` overrides its executable.
A missing service produces an unavailable state without blocking the popout.
Use the calendar icon to open GNOME Calendar for account/event management.
Events load automatically on opening/month changes and update through service
signals; there is no manual refresh button. No-calendar and empty-day states
are distinct; an empty response does not prove a remote account is synced.

## Checks

- `python3 tests/calendar-events.test.py`: updates, recurrence replacement,
  source removal, overlap and exclusive end boundaries.
- `python3 tests/calendar-ui.py`: isolated rendered calendar, leap years,
  month/year boundaries, pointer and keyboard selection, agenda formatting.
  Uses synthetic test-only events and saves `/tmp/bingux-calendar-review.png`.
- `python3 tests/top-bar-controls.py`: search target and notification counter.

Click the month/year heading to choose a month in that year. Click the heading
again to choose a year in the decade, then again to choose a decade in the century.
The arrows page through the current range. Choosing a cell moves down one level;
choosing a month returns to the days and clamps the selected day to that month.
Arrow keys move between picker cells, Enter selects, Page Up/Down changes the
range, and Escape returns one level. Today returns directly to today's day view.
The popup keeps the same height throughout.

Agenda rows open the matching event in GNOME Calendar on click or Enter/Space.
They use the shared instant hover, pressed and focus surface. Event activation
converts the calendar server's source/event/recurrence identifier to Calendar's
UUID format; events without that identifier fall back to opening their date.

In the sidebar, the agenda grows to fit all events for the selected day. Only
the outer sidebar scrolls when the whole calendar exceeds its available height.
The top-bar popout retains its compact scrolling agenda. Verify both short and
busy days with `bash tests/sidebar-notes.sh sidebar-calendar`.
