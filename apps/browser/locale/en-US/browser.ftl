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
# Toast when the engine could not forget visited links after Clear history. $reason is its message.
browser-history-engine-clear-failed = The engine could not forget visited links: { $reason }
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

# Toast shown when Print is chosen and the engine has no print call.
browser-commands-print-unavailable = Printing is not available in this engine yet.

# Toast after Print: the page was saved as a PDF. $path is where.
browser-print-saved = Saved the page as a PDF: { $path }
# Toast when printing fails. $reason is the engine's message.
browser-print-failed = Could not print the page: { $reason }
# Toast when F12 is pressed and the engine has no developer tools.
browser-devtools-unavailable = Developer tools are not available in this engine.
# Toast when F12 is pressed in a private tab: the toolbox keeps its state in the profile.
browser-devtools-private = Developer tools are not available in a private window.
# Toast when F12 is pressed on a page that is not a web page (a blank tab, Settings).
browser-devtools-no-page = There is no web page to inspect here.
# Toast when picture-in-picture is asked for: the engine has no such window.
browser-picture-in-picture-unavailable = Picture-in-picture is not available in this engine yet.
# Toast when Print is chosen on a page that is not a web page (a blank tab, Settings).
browser-print-no-page = There is no web page to print here.

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

## New tab page

browser-new-tab-title = New tab
browser-new-tab-description = Search or enter an address in the bar above.
# The page of a new tab in a private window.
browser-new-tab-private-title = Private browsing
browser-new-tab-private-description = Nothing you do in this window is kept: no history, cookies or site data. Downloads you save stay on this computer.
# Heading of the pages visited last, listed on the new tab page.
browser-new-tab-recent-title = Recently visited

## Error pages
## What stands in the content area when a page could not be shown.

browser-error-unknown-host-title = Can’t find that site
# $host is the site the address names.
browser-error-unknown-host-description = The address { $host } could not be found. Check it for typing mistakes, and that you are connected.
browser-error-refused-title = The site refused the connection
browser-error-refused-description = { $host } is there, but is not accepting connections. It may be down.
browser-error-timeout-title = The site took too long
browser-error-timeout-description = { $host } did not answer in time.
browser-error-offline-title = You are offline
browser-error-offline-description = The connection was lost. Check your network and try again.
browser-error-proxy-title = The proxy refused the connection
browser-error-proxy-description = Check the proxy settings of the system.
browser-error-malformed-title = That address is not valid
browser-error-malformed-description = Check it for typing mistakes.
browser-error-file-title = The file can’t be opened
browser-error-file-description = It may have been moved, or this browser may not be allowed to read it.
browser-error-other-title = The page can’t be shown
browser-error-other-description = Loading { $host } failed.
# $date is the day the certificate stopped being valid, as the engine words it.
browser-error-cert-expired-on = The certificate of { $host } expired on { $date }.
browser-error-cert-expired = The certificate of { $host } has expired.
browser-error-cert-not-yet = The certificate of { $host } is not valid yet.
browser-error-cert-wrong-host = The certificate is for another site, not { $host }.
browser-error-cert-self-signed = { $host } uses a certificate it signed itself.
browser-error-cert-unknown-issuer = The certificate of { $host } was issued by someone this browser does not trust.
browser-error-cert-revoked = The certificate of { $host } was revoked by the one who issued it.
browser-error-cert-other = The certificate of { $host } does not meet this browser’s requirements.
browser-error-crashed-title = This tab crashed
browser-error-crashed-description = The page stopped working and was closed. Reload it to try again. Your other tabs are not affected.
browser-error-killed-title = This tab was stopped
browser-error-killed-description = The system ended the process that showed the page, often because it ran out of memory. Reload it to try again.
browser-error-exited-title = This tab was closed
browser-error-exited-description = The process that showed the page has ended. Reload it to show it again.
browser-error-retry-button = Try again
browser-error-reload-button = Reload tab
# Rows of the certificate details dialog.
browser-error-cert-site-label = Site
browser-error-cert-problem-label = Problem
browser-error-cert-issuer-label = Issued by
browser-error-cert-fingerprint-label = Fingerprint
# Shown in the details when the site cannot be overridden.
browser-error-cert-no-override-description = This site asks to be reached only over a secure connection, so the browser does not let you go on.
browser-error-cert-details-title = Certificate details
browser-error-cert-close-button = Close
browser-error-cert-proceed-button = Proceed (unsafe)
browser-error-cert-title = Your connection is not private
browser-error-cert-back-button = Go back
browser-error-cert-details-button = Details…
# Shown in the details when the person may go on.
browser-error-cert-risk-description = Someone on the network could read or change what you send and receive. Go ahead only if you know why the certificate is wrong.
# $problem is one sentence on what is wrong with the certificate; $host is the site.
browser-error-cert-description = { $problem } Attackers might be trying to steal what you send to { $host }.

## Page prompts
## Dialogs a page opens: alert, confirm, prompt, leave, sign in, select, file.

# Title of the dialog a page shows before it is closed with unsaved changes.
browser-prompt-leave-title = Leave this page?
# Title of a dialog from a page with no address to name.
browser-prompt-page-says-title = This page says
# Title of a dialog from a page. $host is the site.
browser-prompt-host-says-title = { $host } says
browser-prompt-leave-description = Changes you made may not be saved.
browser-prompt-ok-button = OK
browser-prompt-cancel-button = Cancel
browser-prompt-stay-button = Stay on page
browser-prompt-leave-button = Leave
# Title of the sign-in dialog when a proxy asks. $host is its name.
browser-prompt-proxy-title = The proxy { $host } asks you to sign in
# Title of the sign-in dialog. $host is the site.
browser-prompt-sign-in-title = Sign in to { $host }
# $realm is what the site calls the place that asks.
browser-prompt-realm-rejected-description = { $realm } — the name or password was not accepted.
browser-prompt-rejected-description = The name or password was not accepted.
browser-prompt-who-description = The site asks who you are.
browser-prompt-name-placeholder = Name
browser-prompt-password-placeholder = Password
browser-prompt-sign-in-button = Sign in
browser-prompt-select-title = Choose an option
browser-prompt-choose-button = Choose
browser-prompt-file-title = Choose a file
browser-prompt-files-title = Choose files

## Commands
## What the window tells the shared main menu, command palette and shortcuts sheet.

browser-command-new-tab-title = New tab
browser-command-private-window-title = New private window
browser-command-reopen-tab-title = Reopen closed tab
browser-command-close-tab-title = Close tab
browser-command-next-tab-title = Next tab
browser-command-previous-tab-title = Previous tab
browser-command-address-title = Address bar
browser-command-find-title = Find in page
browser-command-reload-title = Reload
browser-command-back-title = Back
browser-command-forward-title = Forward
browser-command-zoom-in-title = Zoom in
browser-command-zoom-out-title = Zoom out
# $percent is the zoom of the tab on show.
browser-command-zoom-reset-title = Reset zoom ({ NUMBER($percent) }%)
browser-command-fullscreen-title = Fullscreen
browser-command-downloads-title = Downloads
browser-command-settings-title = Settings
browser-command-view-source-title = View page source
browser-command-print-title = Save page as PDF
browser-command-devtools-title = Developer tools
# Disabled menu row: the reason is part of the title because the engine has no developer tools to open.
browser-command-devtools-unavailable-title = Developer tools (not available in this engine)
# Disabled command: the engine has no picture-in-picture window to open yet.
browser-command-picture-in-picture-unavailable-title = Picture-in-picture (not available in this engine)
browser-command-go-to-tab-title = Go to a tab
# The keys that go to tab 1 to 8, and the last tab. The modifier is Alt on Linux, Ctrl elsewhere.
browser-command-go-to-tab-keys = Alt+1 to 9

## Menus
## A tab's menu and the page's context menu.

browser-menu-reload-tab = Reload tab
browser-menu-duplicate-tab = Duplicate tab
browser-menu-unpin-tab = Unpin tab
browser-menu-pin-tab = Pin tab
browser-menu-unmute-tab = Unmute tab
browser-menu-mute-tab = Mute tab
browser-menu-close-others = Close other tabs
browser-menu-close-after = Close tabs to the right
browser-menu-open-link = Open link in new tab
browser-menu-copy-link = Copy link address
browser-menu-open-image = Open image in new tab
browser-menu-copy-image = Copy image address
browser-menu-open-video = Open video in new tab
browser-menu-open-audio = Open audio in new tab
browser-menu-copy-media = Copy media address
browser-menu-undo = Undo
browser-menu-redo = Redo
browser-menu-cut = Cut
browser-menu-copy = Copy
browser-menu-paste = Paste
browser-menu-select-all = Select all
browser-menu-back = Back
browser-menu-forward = Forward
browser-menu-reload = Reload
browser-menu-view-source = View page source
browser-menu-copy-page = Copy page address
# Toast after copying an address. $what is link, image, media or page.
browser-menu-copied = { $what ->
        [link] Link address copied
        [image] Image address copied
        [media] Media address copied
       *[page] Page address copied
    }
# Toast when the clipboard cannot be written. $what is as above.
browser-menu-copy-failed = { $what ->
        [link] Could not copy the link address
        [image] Could not copy the image address
        [media] Could not copy the media address
       *[page] Could not copy the page address
    }
# Entry of the page menu for selected text. $engine is the search engine, $text the selection (cut short with an ellipsis).
browser-menu-search-for = Search { $engine } for “{ $text }”

## Tabs
## What a tab is called, the list a phone shows, and what is said when a tab or a pop-up goes.

# The title of the settings tab.
browser-tab-settings-title = Settings
# The title of a tab that has no page yet.
browser-tab-new-title = New tab
# The title of a tab whose page has not given one yet.
browser-tab-loading-title = Loading…
# Title of the list of tabs that a phone shows.
browser-tabs-sheet-title = Tabs
browser-tabs-close-tooltip = Close tab
# Toast shown by Reopen closed tab when nothing was closed.
browser-tabs-none-to-reopen-notification = There is no closed tab to reopen.
# Toast when the engine ends a tab itself. $error is the engine's own words.
browser-tabs-closed-by-engine-notification = A tab was closed by the engine: { $error }
# Said in place of the site when a pop-up came from a page with no address.
browser-tabs-popup-origin-unknown = a page
# Toast when a page opened a window nobody asked for. $site is the page, or a word for it.
browser-tabs-popup-blocked-notification = Blocked a pop-up from { $site }

## Navigation
## The toolbar buttons that move a tab through its history.

browser-nav-back-tooltip = Back
browser-nav-forward-tooltip = Forward
browser-nav-stop-tooltip = Stop loading
browser-nav-reload-tooltip = Reload

## Downloads
## The button, the list and the toasts. Sizes and times are written by bingux_ui::format.

# How far a download is. $done and $total are sizes.
browser-downloads-progress = { $done } of { $total }
# How fast a download goes. $size is the size per second.
browser-downloads-speed = { $size }/s
# $time is how long the download still needs.
browser-downloads-time-left = { $time } left
# The line under a running download: its parts (how far, how fast, how long) in the number given by $count.
browser-downloads-line = { $count ->
        [1] { $first }
        [2] { $first } · { $second }
       *[3] { $first } · { $second } · { $third }
    }
# $done is how far the paused download got.
browser-downloads-paused = Paused · { $done }
browser-downloads-failed = Failed
browser-downloads-cancelled = Cancelled
browser-downloads-pause-tooltip = Pause
browser-downloads-cancel-tooltip = Cancel download
browser-downloads-resume-tooltip = Resume
browser-downloads-retry-tooltip = Retry
browser-downloads-none-title = No downloads
browser-downloads-none-description = Files you save appear here.
browser-downloads-title = Downloads
browser-downloads-clear-button = Clear finished
# Tooltip of the toolbar button. $count is how many downloads run.
browser-downloads-button-tooltip = { $count ->
        [0] Downloads
        [one] Downloads: { $count } running
       *[other] Downloads: { $count } running
    }
browser-downloads-unavailable-tooltip = Downloads are not available in this engine yet
# Toast when a download starts. $name is the file.
browser-downloads-started-notification = Downloading { $name }
# Toast when a download ends. $name is the file.
browser-downloads-finished-notification = { $name } was downloaded
# Button of those toasts: opens the list of downloads.
browser-downloads-show-button = Show

## Settings page
## Rows the page adds to the profile store's own messages above.

browser-settings-remote-off = Off: revoked certificates are caught only when the issuer answers
browser-settings-remote-no-mirror = Security data only, but no mirror is set
browser-settings-remote-mozilla = Security data only, from Mozilla
browser-settings-engine-starting = Starting
browser-settings-engine-stopped = Stopped
browser-settings-engine-running = Running
browser-settings-not-reported = Not reported yet
browser-settings-check-failed = A check failed
browser-settings-check-warn = Some checks warn
browser-settings-check-unknown = Some checks could not be made
browser-settings-check-ok = Every check passes
browser-settings-sandbox-title = Sandbox of each kind of process
browser-settings-checks-title = Other checks
browser-settings-security-title = Security report
browser-settings-security-description = The engine's own check of how it is set up. It finds mistakes in that; it cannot vouch for itself if something has broken into it.
browser-settings-not-checked-description = The engine has not finished checking itself. Open this page again in a moment.
browser-settings-address-title = Address bar
browser-settings-search-label = Search engine
browser-settings-history-label = Browsing history
browser-settings-clear-button = Clear
browser-settings-privacy-title = Privacy
browser-settings-permissions-label = Site permissions
browser-settings-reset-all-button = Reset all
browser-settings-permissions-reset-notification = Site permissions reset
browser-settings-engine-title = Engine
browser-settings-engine-description = Chosen by whoever starts the browser, and read once at start.
browser-settings-user-agent-label = User agent
browser-settings-profile-folder-label = Profile folder
browser-settings-cache-folder-label = Cache folder
browser-settings-downloads-folder-label = Downloads folder
browser-settings-crash-label = Crash reports
browser-settings-crash-description = Kept in this folder, on this computer. They are never sent anywhere.
browser-settings-remote-label = Remote Settings
browser-settings-webrtc-label = WebRTC addresses
browser-settings-processes-label = Content processes
browser-settings-languages-label = Languages
browser-settings-sandbox-label = Required sandbox
browser-settings-title = Settings
# $server is the mirror the browser was started with.
browser-settings-remote-mirror = Security data only, from { $server }
# How many sites have remembered permission choices.
browser-settings-permissions-kept = { $count ->
        [one] { $count } site has choices kept
       *[other] { $count } sites have choices kept
    }

## Profiles
## The window shown when a profile cannot be opened, and the app's name.

browser-profile-in-use-title = This profile is already open
browser-profile-damaged-title = The list of profiles is damaged
browser-profile-too-new-title = A newer Browser set up these profiles
browser-profile-move-waiting-title = Browser is still open from before the update
browser-profile-move-failed-title = Your profile could not be moved
browser-profile-unwritable-title = Browser cannot keep anything
browser-profile-unknown-title = There is no such profile
# $name is the profile.
browser-profile-in-use-description = { $name } is open in another Browser window. Use that window, or close it and try again.
browser-profile-damaged-description = Browser could not read the list of your profiles. Start over makes a new list and keeps the damaged one beside it; your profile folders are not touched.
browser-profile-too-new-description = Update Browser to open these profiles. Nothing was changed.
browser-profile-move-waiting-description = Close the older Browser window, then try again. Your profile is moved once, when nothing is using it.
# $path is the old folder, $problem the system's own words.
browser-profile-move-failed-description = { $path } could not be moved into Browser’s profiles: { $problem }. Nothing was changed.
browser-profile-unwritable-description = { $path } cannot be used: { $problem }.
# $asked is what --profile named.
browser-profile-unknown-description = No profile is called “{ $asked }”.
browser-profile-try-again-button = Try again
browser-profile-start-over-button = Start over
browser-profile-choose-title = Profiles
browser-profile-which-title = Which profile?
browser-profile-which-description = Each profile keeps its own history, cookies and settings.
# The name of the app, in the window title.
browser-title = Browser
# The description of the app, in the About window.
browser-description = Browse the web

## Engine failure
## The page that stands in the whole window when the engine is gone.

# Title when the engine crashed or did something it must not.
browser-engine-crashed-title = Web pages stopped working
# $restart is yes when a new engine may be started and the tabs are kept, no otherwise.
browser-engine-crashed-description = The part of the browser that shows web pages stopped unexpectedly. { $restart ->
        [yes] Your tabs are kept: start again to open them where they were.
       *[no] Close this window and open the browser again.
    }
# $message is the engine's own account.
browser-engine-violation-description = The part that shows web pages did something it must not, so the browser ended it: { $message }. { $restart ->
        [yes] Your tabs are kept: start again to open them where they were.
       *[no] Close this window and open the browser again.
    }
browser-engine-locked-title = This profile is in use
browser-engine-locked-description = Another copy of the browser is using the same profile. Close it, then try again.
browser-engine-sandbox-title = The browser could not start safely
# $message is the engine's own account.
browser-engine-sandbox-description = It could not set up the protection that keeps web pages away from your files, so it did not run: { $message }.
browser-engine-unsafe-title = The profile folder is not safe
# $message names the folder and the rule the engine gave.
browser-engine-unsafe-description = The browser did not start because it does not trust its profile folder: { $message }. Make the folder yours alone (no access for anyone else, no links out of it), then try again.
browser-engine-start-failed-title = The browser could not start
browser-engine-start-failed-description = { $message }. Check how the browser was installed, then try again.
# How the engine ended, for the person who searches for it.
browser-engine-signal-detail = Ended by signal { $signal }
browser-engine-exit-detail = Exit status { $code }
browser-engine-restart-button = Start again
browser-engine-close-button = Close window

## The window title and the page that waits for the engine

# The title of a private window with no page title yet.
browser-window-title-private = Private browsing – Browser
# The title of a window: the page's title, then the app's name. $title is the page's.
browser-window-title-page = { $title } – Browser
# The title of a private window: the page's title, then the kind of window. $title is the page's.
browser-window-title-private-page = { $title } – Private browsing
# What the page area says while the engine is starting.
browser-engine-starting = Starting the engine
