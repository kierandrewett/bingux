// Local Markdown equivalents of Notion's documented blocks and commands.
// Reference and exclusions: docs/notes-slash-commands.md.
var commands = [
    {id: "plain", title: "Text", hint: "Start a plain paragraph", aliases: "text plain paragraph", icon: "format-text-plaintext-symbolic", block: true, markdown: "\u200b"},
    {id: "heading1", title: "Heading 1", hint: "Large heading", aliases: "h1 # heading", iconLabel: "H1", block: true, markdown: "# \u200b"},
    {id: "heading2", title: "Heading 2", hint: "Medium heading", aliases: "h2 ## heading", iconLabel: "H2", block: true, markdown: "## \u200b"},
    {id: "heading3", title: "Heading 3", hint: "Small heading", aliases: "h3 ### heading", iconLabel: "H3", block: true, markdown: "### \u200b"},
    {id: "bullet", title: "Bulleted list", hint: "A list with bullet points", aliases: "bullet list unordered", icon: "view-list-bullet-symbolic", block: true, markdown: "- \u200b"},
    {id: "numbered", title: "Numbered list", hint: "A list with numbered steps", aliases: "num numbered list ordered", icon: "view-list-ordered-symbolic", block: true, markdown: "1. \u200b"},
    {id: "todo", title: "To-do list", hint: "A checklist for tasks", aliases: "todo to-do checkbox task checklist", icon: "checkbox-checked-symbolic", block: true, markdown: "- [ ] \u200b"},
    {id: "table", title: "Table", hint: "Two columns with editable cells", aliases: "table columns rows grid", icon: "x-office-spreadsheet-symbolic", block: true, markdown: "| Column 1 | Column 2 |\n| --- | --- |\n|  |  |\n|  |  |\n\n\u200b", select: "Column 1"},
    {id: "quote", title: "Quote", hint: "Set a quotation apart", aliases: "quote blockquote", icon: "icons/format-quote-symbolic.svg", block: true, markdown: "> \u200b"},
    {id: "divider", title: "Divider", hint: "Separate sections with a line", aliases: "div divider separator horizontal rule", icon: "list-remove-symbolic", block: true, markdown: "---\n\n\u200b"},
    {id: "code", title: "Code block", hint: "Keep code and spacing literal", aliases: "code fenced preformatted", icon: "icons/format-code-symbolic.svg", block: true, markdown: "```\n\u200b\n```"},
    {id: "bold", title: "Bold", hint: "Write bold text", aliases: "bold strong", icon: "format-text-bold-symbolic", markdown: "**Bold text**", select: "Bold text"},
    {id: "italic", title: "Italic", hint: "Write italic text", aliases: "italic italics emphasis", icon: "format-text-italic-symbolic", markdown: "*Italic text*", select: "Italic text"},
    {id: "strikeout", title: "Strikethrough", hint: "Cross out text", aliases: "strike strikethrough strikeout", icon: "format-text-strikethrough-symbolic", markdown: "~~Text~~", select: "Text"},
    {id: "inlineCode", title: "Inline code", hint: "Code within a sentence", aliases: "inline code monospace", icon: "icons/format-code-symbolic.svg", markdown: "`code`", select: "code"},
    {id: "link", title: "Link", hint: "Insert and edit a Markdown link", aliases: "link url web bookmark book", icon: "insert-link-symbolic", input: "[Link text](https://example.com)", select: "https://example.com"},
    {id: "image", title: "Image", hint: "Insert a Markdown image URL or file path", aliases: "image picture photo", icon: "insert-image-symbolic", input: "![Image description](file:///path/to/image.png)", select: "file:///path/to/image.png"},
    {id: "date", title: "Date", hint: "Insert today's date", aliases: "date today timestamp", icon: "x-office-calendar-symbolic"},
    {id: "duplicate", title: "Duplicate block", hint: "Copy the current paragraph below it", aliases: "duplicate copy block", icon: "edit-copy-symbolic"},
    {id: "delete", title: "Delete block", hint: "Remove the current paragraph", aliases: "delete remove block", icon: "edit-delete-symbolic"},
    {id: "heading4", title: "Heading 4", hint: "Fourth-level heading", aliases: "h4 #### heading", iconLabel: "H4", block: true, markdown: "#### \u200b"},
    {id: "heading5", title: "Heading 5", hint: "Fifth-level heading", aliases: "h5 ##### heading", iconLabel: "H5", block: true, markdown: "##### \u200b"},
    {id: "heading6", title: "Heading 6", hint: "Sixth-level heading", aliases: "h6 ###### heading", iconLabel: "H6", block: true, markdown: "###### \u200b"},
];

function search(query) {
    const terms = query.toLowerCase().trim().split(/\s+/).filter(Boolean);
    const found = commands.filter((command) => {
        const words = (command.title + " " + command.aliases).toLowerCase();
        return terms.every((term) => words.includes(term));
    });
    const exact = (entry) => entry.aliases.split(" ").includes(query.toLowerCase().trim());
    return found.filter(exact).concat(found.filter((entry) => !exact(entry)));
}

function token(before) {
    const match = /(?:^|\s)\/([a-z0-9 #_-]{0,48})$/i.exec(before);
    return match ? {start: before.length - match[1].length - 1, query: match[1]} : null;
}
