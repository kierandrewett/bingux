# Calendar

Calendar (`apps/calendar`, program `bingux-calendar`) keeps events and reminders
in plain calendar files. It needs no account and no set-up: the first launch
makes a calendar called Personal and opens on the week that holds today.

The shell's calendar popout (`docs/calendar.md`) is a separate, smaller thing.

## Where the code is

| Part      | Crate                                            | What it holds                                                                                                                                                            |
| --------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| The model | `crates/calendar` (`bingux-calendar-core`)       | Events, iCalendar reading and writing, recurrence, the folder store, reminders, preferences. No UI. Its module docs list the failure modes and the tests that hold them. |
| The views | `crates/ui/src/calendar` (`bingux_ui::calendar`) | The time grid, the month view, the month grid, the date and time pickers. See "Calendar views" in `docs/bingux-ui.md`; the taxonomy's Calendar page shows each.          |
| The app   | `apps/calendar`                                  | Pages and the backend: the window, the header, the sidebar, the editor, calendar management, the views' data.                                                            |

## Views

| Key | View                                                        |
| --- | ----------------------------------------------------------- |
| 1   | Day                                                         |
| 2   | Week (three days on a phone)                                |
| 3   | Month (a small month over the chosen day's list on a phone) |
| 4   | Year                                                        |
| 5   | Agenda: a list from the chosen day on                       |

Overlapping events share the width of their column. All-day events and events
over several days run in a strip above the hours (week and day) or as bars
(month). A calendar's colour tints its events.

Today's column has a line at the time now. The grid opens scrolled to an hour or
two before now when it shows today, and to the start of the working day
otherwise. Press T to go to today and scroll to the time now.

### Making and changing events

- Press N, or the New event button, to add an event for today in the editor.
- Drag over empty time in a week or day to make an event of that length.
- Press a day twice in the month to make an event on it.
- Drag an event to move it, or its top or bottom edge to change when it starts
  or ends. Moves snap to the quarter hour.
- Press an event, or choose it and press Enter, to open the editor.
- Delete removes the chosen event. A toast offers Undo.

### Keyboard

| Key                | Does                                                                                                                                        |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| T                  | Go to today                                                                                                                                 |
| N                  | New event                                                                                                                                   |
| 1 to 5             | Day, Week, Month, Year, Agenda                                                                                                              |
| Left, Right        | Previous, next day, week or month, as the view counts                                                                                       |
| Up, Down           | Week, Day: choose the previous or next event. Month: a week back or ahead, or, with an event chosen, the previous or next event of that day |
| Enter              | Open the chosen event. In the month, with none chosen: choose the first event of the day, or open the day when it has none                  |
| Escape             | Let go of the chosen event, then clear the search                                                                                           |
| Delete             | Delete the chosen event                                                                                                                     |
| Page Up, Page Down | A month back, ahead                                                                                                                         |
| Ctrl+F or /        | Search                                                                                                                                      |

Window keys never act under a menu, a dialog or a sheet (`bingux_ui::nav::modal_open()`),
and a key typed in a text field goes to the field.

## The editor

A popup on a desktop and a sheet on a phone. Title, place, all-day, start and
end (a date picker and a time picker that shows the length), time zone, repeat,
reminders, calendar, availability and notes.

- **Time zone.** Every zone of the IANA database is listed, the cities people
  name first; type in the open list to narrow it. When the event's zone is not
  yours, the editor says what time it is for you.
- **Repeating events.** Changing or deleting one asks whether the change is for
  this event only, this and the following ones, or all of them. "This and the
  following" splits the series in two.
- **Reminders.** Any number, each a time before the start.

## Calendars

The sidebar lists calendars with a check to show or hide each. A calendar's menu
renames, recolours, exports and deletes it. New calendars take one of the
desktop's accent colours. **Import Events** opens a file chooser; a `.ics` file
dropped on the window, or passed on the command line, is imported too, and the
count of added and changed events is shown.

Events are in a folder for each calendar:

```text
$XDG_DATA_HOME/bingux-calendar/calendars/ID/
    displayname      the name
    color            #rrggbb
    UID.ics          one file per event
```

This is the layout `vdirsyncer` and `khal` use, so a sync program that writes
it works with Calendar with nothing else to set up, and Calendar follows
changes other programs make to the files while it runs. A file that is not a
calendar is skipped with a warning, and an event that two files both hold is
shown once.

## Reminders

A reminder is shown through `org.freedesktop.Notifications` when it is due. The
window shows them while it is open; `bingux-calendar --reminders` shows them
with no window (start it at login). Each reminder shows once, even with both
running. A reminder up to 30 minutes late (the machine was asleep) is still shown.

## Preferences

In the main menu: week numbers, the first day of the week (the language's, or
Monday, Saturday or Sunday), 12 or 24-hour time (the language's, or either), the
calendar new events go in, and the reminder new events get. They are kept in
`$XDG_CONFIG_HOME/bingux-calendar/settings.json`.

The first weekday and the clock follow the locale through one place,
`Locale` in `crates/calendar/src/prefs.rs`. When bingux-ui's own `Locale` and
`format::*` land, that type is replaced by them; nothing else in the app reads
the environment.

## Launching

```text
bingux-calendar [--reminders] [FILE.ics | YYYY-MM-DD | today]...
```

One window runs. A second launch gives its argument to the running one.

## Dependencies

Only what Freya needs: the graphics and keyboard libraries the other apps list.
The IANA zone data is compiled in (`chrono-tz`), so events keep their meaning on
a system without `tzdata`.

## Not here yet

CalDAV sync, `webcal://` subscriptions and notification actions that open the
event are not built. Events from a server arrive today by a sync program that
writes the folder layout above.
