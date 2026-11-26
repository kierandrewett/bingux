### Bingux Browser.
### Source language: American English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `browser-`.

## Whether a part of the profile is kept on disk
## $part is `settings` or `history`: what the line is about.

# A private window keeps nothing; its changes go with it.
browser-keeping-private = Changes here last until this window closes
# A window opened without a profile (a test or a snapshot).
browser-keeping-no-profile =
    This window has no profile: { $part ->
        [settings] settings are
       *[history] history is
    } not kept
browser-keeping-disk-full =
    The disk is full: { $part ->
        [settings] settings are
       *[history] history is
    } not being saved
browser-keeping-read-only =
    { $part ->
        [settings] Settings
       *[history] History
    } cannot be saved: the profile folder is read-only
browser-keeping-busy =
    Another program is using the { $part ->
        [settings] settings
       *[history] history
    } file: trying again with the next change
browser-keeping-too-new =
    This profile was used by a newer Browser. Update Browser to keep { $part ->
        [settings] settings
       *[history] history
    }
browser-keeping-damaged =
    { $part ->
        [settings] Settings could not be read. Start over to keep them again
       *[history] History could not be read. Start over to keep it again
    }
# $error is the system's own words.
browser-keeping-failed =
    { $part ->
        [settings] Settings are
       *[history] History is
    } not being saved: { $error }

## Settings page

# Second line of the search engine row.
browser-settings-search-engine-subtitle = Used for words typed in the address bar
# A row shown when the settings file is damaged.
browser-settings-file = Settings file
browser-settings-file-subtitle = Kept aside as it is, and a new one written
# Button that moves a damaged file aside and starts a new one.
browser-start-over = Start over
# Second line of the browsing history row: how many pages the profile keeps.
browser-history-pages =
    { $count ->
        [one] { $count } page kept in this profile
       *[other] { $count } pages kept in this profile
    }
# Second line of the browsing history row in a window that keeps none.
browser-history-not-kept = Not kept in this window
# Under the privacy group: what Clear history does not reach.
browser-history-link-colours-kept = Link colours on pages are kept until the engine can clear them
browser-permissions-footer = Site permissions are kept only while the window is open.

## Toasts

browser-history-cleared = History cleared
browser-history-cleared-link-colours-kept = History cleared. Link colours on pages are kept until the engine can clear them
