### Bingux Calendar.
### Source language: American English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `calendar-`.

# The app's name, in the window title, the header and the About window.
calendar-title = Calendar
# One line in the About window.
calendar-description = Keep track of events and reminders

## Shared words

calendar-cancel = Cancel
calendar-add = Add
calendar-save = Save
calendar-delete = Delete
calendar-rename = Rename
calendar-import = Import
calendar-export = Export
calendar-undo = Undo
calendar-today = Today
calendar-none = None
calendar-never = Never
calendar-next = Next
calendar-previous = Previous
calendar-search = Search

## Errors and results

# A toast when an edit could not be written to disk.
calendar-save-failed = Could not save the change. It is still on screen. Check that the calendar folder can be written to.
# A toast when a calendar could not be changed.
calendar-change-failed = Could not change the calendar. Check that its folder can be written to.
# A toast when there is no calendar to import into.
calendar-import-no-calendar = There is no calendar to import into. Add one first.
# A toast after an export.
calendar-exported = Calendar exported.
calendar-export-failed = Could not export the calendar. Check that the folder can be written to.
# A toast after an event is deleted; Undo follows it.
calendar-event-deleted = Event deleted

## The empty calendar

# A banner when there are no events yet.
calendar-empty-title = Your calendar is empty
calendar-empty-hint = Double-click a day or press N to add an event. Drop a calendar file here to import it.
calendar-empty-add = Add Event
calendar-drop-title = Drop to import events
calendar-drop-description = Calendar files (.ics) are added to your calendar.

## Views and the header

# The heading of the agenda in a wide window. $date (Text) - the day, as written
# The heading of the agenda in a narrow window. $day (Number), $month (Text)
calendar-view-day = Day
calendar-view-week = Week
calendar-view-month = Month
calendar-view-year = Year
calendar-view-agenda = Agenda
calendar-main-menu = Main menu
calendar-calendars = Calendars
calendar-new-event = New event
# A week in one month. $start, $end (Number), $month (Text) - the month and year, or its short form and a year when needed.
calendar-week-range-same-month = { $start } to { $end } { $month }
# A week across two months of one year. $start, $end (Number), $start_month, $end_month (Text), $year (Text) - empty, or " YYYY" when not this year.
calendar-week-range-across-months = { $start } { $start_month } to { $end } { $end_month }{ $year }
# A week across two years. $start, $start_year, $end, $end_year (Number), $start_month, $end_month (Text).
calendar-week-range-across-years = { $start } { $start_month } { $start_year } to { $end } { $end_month } { $end_year }
# The month in a narrow window. $month (Text), $year (Number).
calendar-header-month-compact = { $month } { $year }
# The agenda heading in a narrow window. $day (Number), $month (Text).
calendar-header-agenda-compact = From { $day } { $month }
# The agenda heading in a wide window. $date (Text).
calendar-header-agenda = From { $date }
calendar-event-title = Event
calendar-keyboard-shortcuts = Keyboard shortcuts
calendar-week-numbers = Week numbers
calendar-first-day = First day of the week
calendar-time-format = Time format
calendar-new-events-go-in = New events go in
calendar-new-events-remind = New events remind
calendar-from-language = From the language
calendar-monday = Monday
calendar-saturday = Saturday
calendar-sunday = Sunday
calendar-hour-12 = 12-hour
calendar-hour-24 = 24-hour

## Time and reminders

calendar-all-day = All day
# A time span on one day. $start, $end (Text) - the times
calendar-time-span = { $start } to { $end }
# The start of an event that runs past this day. $time (Text)
calendar-time-from = From { $time }
# The end of an event that began on an earlier day. $time (Text)
calendar-time-until = Until { $time }
calendar-reminder-at-time = At the time of the event
# $count (Number) - minutes before the start
calendar-reminder-minutes = { $count ->
    [one] 1 minute before
   *[other] { $count } minutes before
}
# $count (Number) - hours before the start
calendar-reminder-hours = { $count ->
    [one] 1 hour before
   *[other] { $count } hours before
}
# $count (Number) - days before the start
calendar-reminder-days = { $count ->
    [one] 1 day before
   *[other] { $count } days before
}
# $count (Number) - weeks before the start
calendar-reminder-weeks = { $count ->
    [one] 1 week before
   *[other] { $count } weeks before
}

## Calendars

calendar-add-calendar = Add calendar
calendar-import-events = Import events
calendar-import-events-button = Import Events
calendar-new-calendar = New calendar
calendar-new-calendar-description = Events in it share a color and can be hidden together.
calendar-name-placeholder = Calendar name
calendar-rename-title = Rename calendar
calendar-colour = Color
calendar-files-filter = Calendar files
calendar-all-files-filter = All files
calendar-export-title = Export calendar
# The removal dialog. $name (Text) - the calendar's name
calendar-remove-title = Delete “{ $name }”?
# The removal dialog's note. $count (Number) - its events
calendar-remove-events = { $count ->
    [0] It has no events.
    [one] Its 1 event is deleted with it.
   *[other] Its { $count } events are deleted with it.
}

## Events and the editor

# A title for the editor, and the note of the repeating-event question
calendar-repeats-note = This event repeats.
calendar-save-changes-title = Save changes?
calendar-change-this-title = Change this event?
calendar-delete-this-title = Delete this event?
# The choices for a repeating event. $verb (Text) - Save, Change or Delete
calendar-scope-this = { $verb } this event only
calendar-scope-following = { $verb } this and following events
calendar-scope-all = { $verb } all events
calendar-title-field = Title
calendar-location-field = Location
calendar-location-placeholder = Add a place
calendar-starts = Starts
calendar-ends = Ends
calendar-ends-before-starts = The event ends before it starts.
calendar-all-day-switch = All-day
calendar-time-zone = Time zone
calendar-repeat = Repeat
calendar-repeat-daily = Every day
calendar-repeat-weekdays = Every weekday
calendar-repeat-weekly = Every week
calendar-repeat-fortnightly = Every 2 weeks
calendar-repeat-monthly = Every month
calendar-repeat-yearly = Every year
calendar-repeat-ends = Ends repeating
calendar-repeat-end-on = On a date
calendar-repeat-end-after = After a set number of times
calendar-calendar-field = Calendar
calendar-reminder = Reminder
calendar-show-as = Show as
calendar-busy = Busy
calendar-free = Free
calendar-notes = Notes
calendar-url = URL
calendar-delete-event = Delete event
calendar-change = Change
calendar-delete-ellipsis = Delete…
calendar-rename-ellipsis = Rename…
calendar-export-ellipsis = Export…
calendar-close = Close
# Import and read errors, with the file name.
calendar-read-failed = Could not read { $name }. Check that you can open the file.
calendar-not-calendar = Could not import { $name }. It is not a calendar file.
calendar-no-events-to-import = { $name } has no events to import.
# A toast and a line of the import dialog. $count (Number) - how many events
calendar-imported = { $count ->
    [one] Imported { $count } event.
   *[other] Imported { $count } events.
}
# The import dialog's line with the first thing that was skipped. $message (Text) - what was imported, $warning (Text) - what the file read said
calendar-import-note = { $message } { $warning }
# Agenda day headings.
calendar-yesterday = Yesterday
calendar-tomorrow = Tomorrow
calendar-nothing-coming-up = Nothing coming up
# $days (Number) - how many days the agenda looks ahead
calendar-no-events-in-days = { $days ->
    [one] There are no events in the next day.
   *[other] There are no events in the next { $days } days.
}
calendar-add-event = Add event
calendar-no-events = No events
calendar-no-results = No results
calendar-nothing-matches = Nothing matches “{ $query }”.

## Header menu, reminders and the agenda's button

# The toast of a reminder with no notification service. $title (Text) - the event, $body (Text) - when and where
calendar-reminder-toast = { $title }. { $body }

## The words of a reminder notification

calendar-reminder-untitled = Event
calendar-reminder-today = Today
calendar-reminder-tomorrow = Tomorrow
# $count (Number) - days until an all-day event
calendar-reminder-in-days = { $count ->
    [one] In 1 day
   *[other] In { $count } days
}
calendar-reminder-now = Now
# $count (Number) - minutes until it starts
calendar-reminder-in-minutes = { $count ->
    [one] In 1 minute
   *[other] In { $count } minutes
}
# $count (Number) - minutes since it started
calendar-reminder-started-ago = { $count ->
    [one] Started 1 minute ago
   *[other] Started { $count } minutes ago
}
# $time (Text) - the start, such as 13:00
calendar-reminder-today-at = Today at { $time }
# $day (Text) - the date, such as Wed 12 Mar; $time (Text) - the start
calendar-reminder-day-at = { $day } at { $time }
# $when (Text) - when it starts; $place (Text) - where
calendar-reminder-with-place = { $when }, { $place }

## Things read in a forgiving way from a calendar file

# $named (yes or no) - whether the event has a title; $title (Text) - the title
calendar-warning-no-start = { $named ->
    [yes] “{ $title }” has no start and was skipped.
   *[no] An event has no start and was skipped.
}
# $named (yes or no) - whether the event has a title; $title (Text) - the title
calendar-warning-unreadable-start = { $named ->
    [yes] “{ $title }” has a start that could not be read and was skipped.
   *[no] An event has a start that could not be read and was skipped.
}
# $named (yes or no) - whether the event has a title; $title (Text) - the title
calendar-warning-stray-exception = { $named ->
    [yes] “{ $title }” changes an event that is not in the file; it is kept on its own.
   *[no] An event changes one that is not in the file; it is kept on its own.
}
# $zone (Text) - the zone name the file gave
calendar-warning-unknown-zone = The time zone “{ $zone }” is not known; its times are read in your time zone.
# $file (Text) - a file name
calendar-warning-file-unreadable = { $file } could not be read.
# $file (Text) - a file name; $detail (Text) - the reader's own line
calendar-warning-file-not-calendar = { $file } is not a calendar file ({ $detail }).
# $file (Text) - a file name; $first (Text) - the file that already has the event
calendar-warning-file-repeats = { $file } repeats an event that { $first } already has, so it is left out.
# $file (Text) - a file name; $warning (Text) - the sentence for what was found in it
calendar-warning-in-file = { $file }: { $warning }
# The value of the stepper after "Ends after". $count (Number) - how many repeats
calendar-repeat-times = { $count ->
    [one] { $count } time
   *[other] { $count } times
}
calendar-new-calendar-ellipsis = New calendar…
calendar-import-events-ellipsis = Import events…
# The button at the end of the agenda. $count (Number) - how many more days it adds
calendar-show-more-days = { $count ->
    [one] Show { $count } more day
   *[other] Show { $count } more days
}

# The text of the keyboard shortcuts dialog, one line to a shortcut.
calendar-keyboard-shortcuts-body =
    T  Go to today
    N  New event
    1 to 5  Day, Week, Month, Year, Agenda
    Left, Right  Previous, next
    Up, Down  A week back, ahead (Month); choose an event (Week, Day)
    Enter  Open the chosen event, or choose the first of the day (Month)
    Escape  Let go of the chosen event, or clear the search
    Page Up, Page Down  A month back, ahead
    Ctrl+F or /  Search
    Delete  Delete the chosen event
    Ctrl+Shift+?  About Calendar

    In a week or a day, drag over empty time to make an event, drag an event to
    move it, and drag its top or bottom edge to change when it starts or ends.
    Press a day twice to make an event on it.
