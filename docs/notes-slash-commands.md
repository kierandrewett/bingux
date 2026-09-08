# Sidebar note slash commands

Type `/` at the start of a line or after a space. Continue typing to filter the
menu. Up and Down select a command; Enter or Tab inserts it. Escape leaves the
slash text in the note. Click an entry to insert it. The editor keeps keyboard
focus. URLs, file paths and fenced code do not open the menu.

The menu supplies local Markdown formats. It does not connect to a Notion
workspace. Notes continue to use `sidebar-notes.ini`; there is no storage migration.

## Implemented commands

| Commands | Result |
| --- | --- |
| `/text`, `/plain` | Plain paragraph |
| `/h1` to `/h6`, `/#` to `/######` | Heading levels 1 to 6 |
| `/bullet`, `/num`, `/todo` | Bulleted, numbered or checkbox list |
| `/table` | Editable two-column table with a header and two rows |
| `/quote`, `/div`, `/code` | Quote, divider or fenced code |
| `/bold`, `/italic`, `/strike`, `/inline code` | Selected sample text with that format; typing replaces it |
| `/link`, `/book` | Editable Markdown link; replace the selected URL and press Enter |
| `/image` | Editable Markdown image; replace the selected file path or URL and press Enter |
| `/date` | Current local date |
| `/duplicate`, `/delete` | Duplicate or remove the current paragraph |

Insertion uses the existing document editor and grouped undo. Ctrl+Z restores the
slash command. Tables and text formats remain editable after saving and reopening.
A link uses a normal hyperlink, not Notion's bookmark preview. A date does not
schedule a reminder. Images use Qt's document image support, not an upload service.

## Notion reference inventory

Checked on 8 September 2026. Notion publishes a
[content-type list](https://www.notion.com/help/writing-and-editing-basics) and a
[slash-command reference](https://www.notion.com/help/keyboard-shortcuts).
The list below records their command families, including those that need features
outside this Markdown editor. It is a reference inventory, not a claim of feature
parity or an export of a signed-in Notion menu.

| Family | Published entries |
| --- | --- |
| Basic | Text, page, to-do, headings 1-3, table, bullet, numbered list, toggle, quote, divider, link to page, callout |
| Inline | Person/page mention, date/reminder, equation, emoji |
| Media | Image, video, audio, file, code, web bookmark |
| Named embeds | Google Drive, tweet, GitHub Gist, Google Maps, Framer, Invision, PDF, Figma, Loom, Typeform, CodePen, Whimsical, generic embed |
| Advanced | Equation, button/template, breadcrumb, table of contents |
| Actions | Duplicate, move to, delete, comment, turn into, text/background colour |

The keyboard reference also gives `/math` and `/latex` aliases for equations,
`/bread` for breadcrumbs, and `/toc` for contents. Formatting includes bold,
italic, underline, strikethrough and inline code.

Notion's newer documentation also lists database views: table, board, calendar,
timeline, gallery, list, form, chart, map and dashboard.
See [working with views](https://developers.notion.com/guides/data-apis/working-with-views).
[Toggle headings](https://www.notion.com/releases/2021-12-23) add collapsible heading
levels 1-3.

Workspace pages, mentions, databases, reminders, playable embeds, collaborative
comments, automation buttons, synced content and collapsible layouts require their
own data and rendering support. Underline and colours also need a storage format
that preserves them. These are not presented as working commands in this menu.

## Verification

Use the same Quickshell and QML plugin environment as the desktop:

```sh
QUICKSHELL_BIN=/path/to/desktop-quickshell tests/sidebar-notes.sh notes-slash
BINGUX_NOTES_NATIVE=1 QUICKSHELL_BIN=/path/to/desktop-quickshell tests/sidebar-notes.sh notes-slash
```

The test runner redirects note settings to a temporary file. It checks keyboard
navigation, pointer activation, empty results, dismissal, literal slashes, actual
formatting, editable table cells, grouped undo, persistence and menu bounds.
Run `sidebar-notes` and `notes-headings` through the same runner for regressions.

Heading formatting uses `DocumentEdit` from `Bingux.Text`. The native edit groups
fragment replacement and spacing before Qt updates the layout. This prevents a
partially removed document from changing the scroll position. The helper also
provides the native undo group.

`tests/sidebar-notes.sh notes-scroll` checks slash commands, typed Markdown and
context-menu headings in a scrolled note. The original failure moved the caret by
130 px and temporarily reduced document height from 605 px to 207 px. The fixed
path preserves document height during insertion and moves the heading only by its
14 px top margin. Offscreen and isolated Wayland runs pass, including continued
typing and Enter. Updating this native plugin requires restarting Quickshell.
