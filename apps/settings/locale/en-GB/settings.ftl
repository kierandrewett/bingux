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
