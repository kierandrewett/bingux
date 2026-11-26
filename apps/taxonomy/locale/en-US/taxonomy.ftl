### Bingux Taxonomy: the app that shows every bingux-ui component.
### Source language: American English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `taxonomy-`.

# The app's name, in the window title and the About window.
taxonomy-title = Taxonomy

## Controls > Buttons: the stepper

# The value of a stepper whose unit agrees with the count. $count (Number) - the value
taxonomy-stepper-items = { $count ->
    [one] { $count } item
   *[other] { $count } items
}

## Foundations > Localisation: the live demo of bingux_ui::i18n

taxonomy-l10n-language = Language
# A choice of the language switch: follow the session's language.
taxonomy-l10n-system = System
# A choice that writes every message with accents, to find strings that are
# not localised.
taxonomy-l10n-accented = Accented pseudo-locale
# A choice that writes every message right to left.
taxonomy-l10n-rtl = Right-to-left pseudo-locale
# The language the window is shown in now, such as "de".
taxonomy-l10n-shown = Shown in
# The languages a message is looked up in, in order.
taxonomy-l10n-chain = Fallback order
taxonomy-l10n-direction = Reading direction
taxonomy-l10n-direction-ltr = Left to right
taxonomy-l10n-direction-rtl = Right to left
# Whether the taxonomy's catalog matches its code.
taxonomy-l10n-check = Catalog check
taxonomy-l10n-check-clean-status = No problems
# The check needs the source tree, which an installed copy does not have.
taxonomy-l10n-check-unavailable-status = Needs the source tree
# $count (Number) - how many problems the check found
taxonomy-l10n-check-problems-status = { $count ->
    [one] { $count } problem
   *[other] { $count } problems
}
taxonomy-l10n-plurals-title = Plurals
# A count of files: each language has its own plural forms.
# $count (Number) - how many files
taxonomy-l10n-files = { $count ->
    [0] No files
    [one] { $count } file
   *[other] { $count } files
}
# A name in another script inside a sentence.
taxonomy-l10n-isolation = A name in a sentence
# $name (String) - a file name, here in Hebrew
taxonomy-l10n-moved = Moved “{ $name }” to the Bin.
taxonomy-l10n-number = A number
# $size (Number) - a size with one decimal
taxonomy-l10n-size = { $size } MB free
# A message the German stub has not translated, so it shows in English.
taxonomy-l10n-fallback = Not yet translated
taxonomy-l10n-fallback-sample = This line falls back to American English.
taxonomy-l10n-shortcut = A shortcut
# A message from bingux-ui's own catalog.
taxonomy-l10n-ui-string = A bingux-ui message
# A button whose tooltip is an attribute of its message.
taxonomy-l10n-attribute = A tooltip attribute
taxonomy-l10n-save = Save
    .tooltip = Save the document
taxonomy-l10n-note-description = The switch changes every window at once. Pages keep their place, and only the text changes.

### The chrome of the taxonomy window: the display switcher, the search results and the toasts.

taxonomy-description = Every Bingux UI component, icon and token, running
taxonomy-search-results-title = Results
taxonomy-search-empty-title = Nothing matches
taxonomy-search-empty-description = Try a Rust name such as Switch or Icon
# $doc (String) - the path of the documentation anchor that was copied
taxonomy-copied-toast = Copied { $doc }
# $covers (String) - the names of the items the entry covers, joined with commas
taxonomy-covers = Covers { $covers }
taxonomy-switcher-title = Display
taxonomy-switcher-tooltip = Theme, accent, density, scale and contrast
taxonomy-choice-theme = Theme
taxonomy-choice-accent = Accent
taxonomy-choice-density = Density
taxonomy-choice-scale = Scale
taxonomy-choice-text-size = Text size
taxonomy-choice-strings = Strings
taxonomy-choice-contrast = Contrast
# $name (String) - the theme's name, such as Dark
taxonomy-choice-theme-option = Theme: { $name }
taxonomy-choice-accent-system = Accent: system
# $name (String) - the accent's name, such as blue
taxonomy-choice-accent-option = Accent: { $name }
# $name (String) - the density's name, such as Compact
taxonomy-choice-density-option = Density: { $name }
# $scale (Number) - a window scale in percent
taxonomy-choice-scale-option = Scale: { $scale }%
# $scale (Number) - a text scale in percent
taxonomy-choice-text-option = Text: { $scale }%
taxonomy-choice-strings-plain = Strings: plain
taxonomy-choice-strings-pseudo = Strings: pseudo
taxonomy-choice-contrast-standard = Contrast: standard
taxonomy-choice-contrast-high = Contrast: high
