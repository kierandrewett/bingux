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

## Zoom

# The toolbar button that puts the page zoom back to 100 %. $percent is the zoom.
browser-zoom-chip = { NUMBER($percent) }%

## Address bar

# Placeholder of the empty address field on a desktop.
browser-address-placeholder = Search or enter an address
# Placeholder of the empty address field on a phone, which has less room.
browser-address-compact-placeholder = Search or address
# First suggestion when what was typed is an address. $address is the address as the bar shows it.
browser-address-go-to = Go to { $address }
# First suggestion when what was typed is words. $engine is the name of the search engine, $words what was typed.
browser-address-search-for = Search { $engine } for “{ $words }”

## Commands

# Toast shown when Print is chosen: the engine cannot print yet.
browser-commands-print-unavailable = Printing is not available in this engine yet.

## Site information
## The card under the connection button.

# Title of the connection summary when no page is open.
browser-site-info-blank-title = No page
browser-site-info-blank-description = There is nothing to describe yet.
browser-site-info-internal-title = A page of this browser
browser-site-info-internal-description = This page is part of the browser and sends nothing to the network.
browser-site-info-local-title = A file on this computer
browser-site-info-local-description = The page was read from your disk, not from the network.
browser-site-info-mixed-title = Connection is secure, but some content is not
browser-site-info-mixed-description = Images or media on this page were loaded without encryption. Someone on the network could see or replace them.
browser-site-info-secure-title = Connection is secure
browser-site-info-secure-description = What you send to this site is private on its way there.
browser-site-info-verified-description = What you send to this site is private, and the organization behind it was verified.
# Also the title when the page went on past a certificate warning.
browser-site-info-insecure-title = Connection is not secure
browser-site-info-insecure-description = Anyone on the network could read this page and what you send to it. Do not enter passwords or card numbers here.
browser-site-info-active-mixed-description = Scripts on this page were loaded without encryption. Someone on the network could change what the page does.
browser-site-info-certificate-description = You went on past a warning about this site’s certificate. Do not enter passwords or card numbers here.
browser-site-info-connecting-title = Connecting
browser-site-info-connecting-description = The connection is being made; nothing is known about it yet.

# Heading of the group that names the site, and the labels of its two rows.
browser-site-info-site-title = Site
browser-site-info-host-label = Site
browser-site-info-address-label = Address
browser-site-info-permissions-title = Permissions
browser-site-info-permissions-footer = Choices you made for this site. Reset one to be asked again.
# A remembered choice for a permission.
browser-site-info-allowed = Allowed
browser-site-info-blocked = Blocked
browser-site-info-reset-button = Reset

## Permissions
## What a page asks to use. Each kind has a label (a row of the page info) and
## a want (part of the question: "example.com wants to know your location").

browser-permissions-geolocation-label = Know your location
browser-permissions-geolocation-want = know your location
browser-permissions-notifications-label = Show notifications
browser-permissions-notifications-want = show notifications
browser-permissions-camera-label = Use your camera
browser-permissions-camera-want = use your camera
browser-permissions-microphone-label = Use your microphone
browser-permissions-microphone-want = use your microphone
browser-permissions-screen-label = Share your screen
browser-permissions-screen-want = share your screen
browser-permissions-clipboard-label = See what you copied
browser-permissions-clipboard-want = see what you copied
browser-permissions-midi-label = Use your MIDI devices
browser-permissions-midi-want = use your MIDI devices
browser-permissions-autoplay-label = Play sound on its own
browser-permissions-autoplay-want = play sound on its own
browser-permissions-storage-label = Keep data when space is low
browser-permissions-storage-want = keep data when space is low
browser-permissions-popups-label = Open pop-up windows
browser-permissions-popups-want = open pop-up windows
# The question. $host is the site, $wants what it asks for, joined as a list ("a, b, and c").
browser-permissions-prompt = { $host } wants to { $wants }
# Said in place of $wants when the page asked for nothing the browser can name.
browser-permissions-prompt-nothing = do something
browser-permissions-block-button = Block
browser-permissions-allow-once-button = Allow this time
browser-permissions-allow-button = Allow

## Find in page

# Placeholder of the find field.
browser-find-placeholder = Find in page
browser-find-match-case = Match case
browser-find-highlight-all = Highlight all
# Toast shown when the engine cannot search a page.
browser-find-unavailable = Find in page is not available in this engine yet.
