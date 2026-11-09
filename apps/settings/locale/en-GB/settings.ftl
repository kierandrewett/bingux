### Bingux Settings.
### Source language: British English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `settings-`.
###
### Search keywords (`settings_index.rs`) stay literal in the code: they are
### search data that people type, not text the window shows.

# The app's name, in the window title, the header and the About window.
settings-title = Settings
# One line in the About window.
settings-description = Change Bingux and system settings.

## Pages (sidebar, headers and search)

# The title of each page, in the sidebar, the header and search results.
settings-page-network = Network
settings-page-bluetooth = Bluetooth
settings-page-appearance = Appearance
settings-page-privacy = Privacy
settings-page-accessibility = Accessibility
settings-page-desktop = Desktop and dock
settings-page-search = Search
settings-page-displays = Displays
settings-page-sound = Sound
settings-page-power = Power
settings-page-keyboard = Keyboard
settings-page-mouse = Mouse and touchpad
settings-page-datetime = Date and time
settings-page-users = Users
settings-page-applications = Applications
settings-page-about = About

## Window shell (the sidebar, the command palette and the shortcuts sheet)

# The group of links under a page, for the pages that go with it.
settings-related-title = Related
# Commands in the palette and the shortcuts sheet.
settings-command-search = Search settings
settings-command-back = Back
settings-command-regions = Move between the sidebar and the page

## About

# The system facts in the About page, with their labels.
settings-about-spec-os = Operating system
settings-about-spec-version = Version
settings-about-spec-kernel = Kernel
settings-about-spec-processor = Processor
settings-about-spec-memory = Memory
settings-about-spec-graphics = Graphics
settings-about-spec-display = Display
settings-about-spec-uptime = Uptime
settings-about-spec-desktop = Desktop
settings-about-spec-settings-version = Settings version
# A value a fact could not be read for.
settings-about-unknown = Unknown
settings-about-not-running = Not running
# A memory size in gibibytes, such as "15.6 GiB".
settings-about-memory-gib = { $gib } GiB
# The first connected display's mode, and how many more are connected.
settings-about-display-more = { $first } (+{ $more } more)
# The text copied by "Copy system info".
settings-about-copy-device-name = Device name: { $name }
settings-about-copy-model = Hardware model: { $model }
settings-about-copy-row = { $title }: { $value }
settings-about-copy-storage = Storage: { $used } used of { $total }
settings-about-copy-storage-unknown = Storage: Unknown
# Toasts after copying the system info.
settings-about-copied = Copied
settings-about-copy-failed = Could not copy to the clipboard. Try again.
# The device name row, its rename sheet and its validation messages.
settings-about-device-name = Device name
settings-about-rename = Rename
settings-about-name-empty = Enter a device name.
settings-about-name-too-long = Use 63 characters or fewer.
settings-about-name-invalid = Use only letters, numbers and hyphens. Do not start or end with a hyphen.
settings-about-name-help = Other devices on your network see this name. Use letters, numbers and hyphens.
# The storage gauge.
settings-about-storage = Storage
settings-about-storage-used-unit = { $unit } used
settings-about-storage-total = { $total } total
settings-about-storage-used-of = { $used } used of { $total }
# Labels and menu items for the rest of the page.
settings-about-capacity = Capacity
settings-about-copy = Copy
settings-about-copy-system-info = Copy system info
settings-about-system-details = System details

## Accessibility

# The groups of switches on the Accessibility page, and their labels.
settings-accessibility-seeing = Seeing
settings-accessibility-screen-reader = Screen reader
settings-accessibility-large-text = Large text
settings-accessibility-high-contrast = High contrast
settings-accessibility-reduce-animations = Reduce animations
settings-accessibility-cursor-size = Cursor size
settings-accessibility-overlay-scrollbars = Overlay scrollbars
settings-accessibility-typing = Typing
settings-accessibility-on-screen-keyboard = On-screen keyboard
settings-accessibility-sticky-keys = Sticky keys
settings-accessibility-slow-keys = Slow keys
settings-accessibility-bounce-keys = Bounce keys
settings-accessibility-pointing = Pointing
settings-accessibility-mouse-keys = Mouse keys
settings-accessibility-locate-pointer = Locate pointer

## Appearance

# The wallpaper, font, accent and colour scheme on the Appearance page.
settings-appearance-text = Text
settings-appearance-font = Font
settings-appearance-text-size = Text size
settings-appearance-themes = Themes
settings-appearance-icons = Icons
settings-appearance-cursor = Cursor
settings-appearance-accent-colour = Accent colour
settings-appearance-accent-label = Accent colour { $name }
settings-appearance-colour-scheme = Colour scheme
settings-appearance-scheme-default = Default
settings-appearance-scheme-light = Light
settings-appearance-scheme-dark = Dark
settings-appearance-gsettings-unavailable = Accent colour and colour scheme are not available on this system.
settings-appearance-wallpaper = Wallpaper
settings-appearance-choose-wallpaper = Choose a wallpaper
settings-appearance-filter-pictures = Pictures
settings-appearance-choose-picture = Choose picture
settings-appearance-use-system-wallpaper = Use system wallpaper
settings-appearance-system-wallpaper = System wallpaper
settings-appearance-add-picture = Add picture…
settings-appearance-search-pictures = Search pictures
settings-appearance-source-system = System
settings-appearance-source-yours = Your pictures
settings-appearance-no-pictures = No pictures. Add one to use it as the wallpaper.
settings-appearance-no-matching-pictures = No matching pictures
settings-appearance-font-not-installed = Could not set the font “{ $family }”. Choose an installed font.
settings-appearance-try-again = Try again
settings-appearance-system-default = System default
settings-appearance-font-name-copied = Font name copied
settings-appearance-font-copy-failed = Could not copy the font name. Try again.
settings-appearance-use-this-font = Use this font
settings-appearance-copy-name = Copy name
settings-appearance-no-matching-fonts = No matching fonts
settings-appearance-filter-all = All
settings-appearance-filter-sans-serif = Sans serif
settings-appearance-filter-serif = Serif
settings-appearance-filter-monospace = Monospace
settings-appearance-filter-other = Other
settings-appearance-search-fonts = Search fonts

## Applications

# Default applications, the installed list and the uninstall dialog.
settings-applications-kind-web = Web
settings-applications-kind-mail = Mail
settings-applications-kind-calendar = Calendar
settings-applications-kind-files = Files
settings-applications-kind-text = Text
settings-applications-kind-documents = Documents
settings-applications-kind-photos = Photos
settings-applications-kind-music = Music
settings-applications-kind-video = Video
settings-applications-uninstalled = “{ $name }” uninstalled
settings-applications-uninstall-failed = Could not uninstall “{ $name }”. Try again.
settings-applications-uninstall-body-source = “{ $name }” is installed from { $source }.
settings-applications-uninstall-body-system = “{ $name }” is installed on this system.
settings-applications-uninstall-title = Uninstall “{ $name }”?
settings-applications-cancel = Cancel
settings-applications-uninstall = Uninstall
settings-applications-not-set = Not set
settings-applications-open = Open
settings-applications-show-details = Show details
settings-applications-make-default = Make default
settings-applications-search = Search
settings-applications-filter-all = All
settings-applications-sort-name = Name
settings-applications-sort-size = Size
settings-applications-filter-and-sort = Filter and sort
settings-applications-source = Source
settings-applications-sort-by = Sort by
settings-applications-installed = Installed
settings-applications-showing = Showing { $kind }
settings-applications-largest-first = Largest first
settings-applications-showing-largest-first = Showing { $kind }, largest first
settings-applications-no-matching = No matching applications
settings-applications-default-applications = Default applications
settings-applications-unknown = Unknown
settings-applications-no-longer-installed = This application is no longer installed.
settings-applications-open-by-default = Open by default
settings-applications-try-again = Try again
settings-applications-not-default-application = Not available as a default application.
settings-applications-default = Default
settings-applications-not-default = Not default
settings-applications-details = Details
settings-applications-version = Version
settings-applications-size = Size
settings-applications-installed-from = Installed from
settings-applications-location = Location
settings-applications-category = Category
settings-applications-startup = Startup
settings-applications-start-at-login = Start at login
settings-applications-notifications = Notifications
settings-applications-show-notifications = Show notifications
settings-applications-show-on-lock-screen = Show on the lock screen
settings-applications-play-notification-sounds = Play notification sounds
settings-applications-removes-package = Removes the package this application came from.
settings-applications-deletes-files = Deletes the files this application installed.
settings-applications-remove-from-computer = Remove from this computer
settings-applications-file-types = File types
settings-applications-supported-file-types = Supported file types
settings-applications-other-file-types = Other file types
settings-applications-could-not-find-file-type = Could not find this file type.
settings-applications-default-application = Default application
settings-applications-has-default = Has a default
settings-applications-sort-application = Application
settings-applications-no-matching-file-types = No matching file types
settings-applications-category-images = Images
settings-applications-category-audio = Audio
settings-applications-category-video = Video
settings-applications-category-documents = Documents
settings-applications-category-archives = Archives
settings-applications-category-fonts = Fonts
settings-applications-category-code = Code
settings-applications-category-other = Other

## Desktop

# The Desktop and dock page: removable media, privacy, dock, sidebar and Control Centre.
settings-desktop-tile-vpn = VPN
settings-desktop-tile-dnd = Do not disturb
settings-desktop-tile-night-light = Night light
settings-desktop-tile-power-mode = Power mode
settings-desktop-tile-keep-awake = Keep awake
settings-desktop-unreadable = Could not read the desktop settings. Restart the desktop and try again.
settings-desktop-removable-media = Removable media
settings-desktop-mount-when-plugged = Mount drives when they are plugged in
settings-desktop-open-after-mount = Open the folder after mounting
settings-desktop-run-programs = Run programs from removable media
settings-desktop-privacy = Privacy
settings-desktop-show-public-ip = Show my public IP address
settings-desktop-show-on-sign-in = Show on sign-in screen
settings-desktop-picture-and-wallpaper = Picture and wallpaper
settings-desktop-remember-recent = Remember recent files
settings-desktop-forget-recent-after = Forget recent files after
settings-desktop-introduction = Introduction
settings-desktop-shortcuts-tour = Shortcuts and shell tour
settings-desktop-shown-at-sign-in = Shown at first sign-in
settings-desktop-show-introduction = Show introduction
settings-desktop-introduction-failed = Could not show the introduction. Restart the desktop and try again.
settings-desktop-recent-never = Never
settings-desktop-recent-one-day = 1 day
settings-desktop-recent-days = { $count } days
settings-desktop-icon-size = Icon size
settings-desktop-icon-size-px = { $px } px
settings-desktop-try-again = Try again
settings-desktop-dock = Dock
settings-desktop-show-dock = Show dock
settings-desktop-alignment = Alignment
settings-desktop-align-left = Left
settings-desktop-align-centre = Centre
settings-desktop-align-right = Right
settings-desktop-align-left-menu = Align left
settings-desktop-align-centre-menu = Align centre
settings-desktop-align-right-menu = Align right
settings-desktop-sidebar = Sidebar
settings-desktop-show-sidebar = Show sidebar
settings-desktop-sidebar-side = Side of the screen
settings-desktop-sidebar-auto = Automatic
settings-desktop-sidebar-left = Left
settings-desktop-sidebar-right = Right
settings-desktop-sidebar-top = Top
settings-desktop-dock-mouse-actions = Dock mouse actions
settings-desktop-click = Click
settings-desktop-click-toggle = Show or hide windows
settings-desktop-click-focus = Bring the window to the front
settings-desktop-click-launch = Open a new window
settings-desktop-middle-click = Middle click
settings-desktop-middle-launch = Open a new window
settings-desktop-middle-close = Close all its windows
settings-desktop-middle-none = Do nothing
settings-desktop-scroll = Scroll
settings-desktop-scroll-cycle = Switch between windows
settings-desktop-scroll-none = Do nothing
settings-desktop-scroll-direction = Scroll direction
settings-desktop-scroll-natural = Like a touchscreen
settings-desktop-scroll-reverse = Reversed
settings-desktop-top-bar = Top bar
settings-desktop-show-system-usage = Show system usage
settings-desktop-show-clock-seconds = Show seconds in the clock
settings-desktop-control-centre = Control Centre
