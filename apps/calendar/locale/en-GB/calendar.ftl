### Bingux Calendar.
### Source language: British English. Conventions: docs/bingux-ui.md,
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
calendar-new-calendar-description = Events in it share a colour and can be hidden together.
calendar-name-placeholder = Calendar name
calendar-rename-title = Rename calendar
calendar-colour = Colour
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
calendar-new-event-title = New event
calendar-close = Close
# Import and read errors, with the file name.
calendar-read-failed = Could not read { $name }. Check that you can open the file.
calendar-not-calendar = Could not import { $name }. It is not a calendar file.
calendar-no-events-to-import = { $name } has no events to import.
calendar-imported-one = Imported 1 event.
calendar-imported-many = Imported { $count } events.
calendar-no-calendar-to-add = There is no calendar to add the event to.
# Quick add placeholder and hint.
calendar-quick-hint = Say what, when and where. For example: Lunch with Sam at Nando’s tomorrow 1pm.
calendar-quick-placeholder = Lunch with Sam tomorrow 1pm
calendar-more-options = More Options
# Agenda day headings.
calendar-yesterday = Yesterday
calendar-tomorrow = Tomorrow
calendar-nothing-coming-up = Nothing coming up
calendar-no-events-in-days = There are no events in the next { $days } days.
calendar-add-event = Add event
calendar-no-events = No events
calendar-no-results = No results
calendar-nothing-matches = Nothing matches “{ $query }”.
