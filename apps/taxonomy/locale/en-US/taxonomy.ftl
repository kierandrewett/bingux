### Bingux Taxonomy: the app that shows every bingux-ui component.
### Source language: American English. Conventions: docs/bingux-ui.md,
### "Localisation". Every ID starts with `taxonomy-`.

# The app's name, in the window title and the About window.
taxonomy-title = Taxonomy

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
