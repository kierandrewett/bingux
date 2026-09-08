#include <QGuiApplication>
#include <QDebug>
#include "document-spacing.h"

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QTextDocument doc;
    doc.setMarkdown("# Heading\n\nBody text\n\n## Next heading\n\nLast paragraph");
    doc.clearUndoRedoStacks(); // TextEdit starts a loaded note with no edit history.
    const auto original = doc.toMarkdown();
    DocumentSpacing::apply(&doc);
    auto check = [](bool value, const char *message) {
        if (!value) qFatal("%s", message);
    };
    check(doc.toMarkdown() == original, "Spacing changed Markdown");
    check(!doc.isUndoAvailable(), "Initial spacing created an undo action");
    check(doc.begin().blockFormat().lineHeight() == 115, "Line spacing must be 1.15");
    check(doc.begin().next().blockFormat().bottomMargin() == 0, "Body paragraphs must not gain extra gaps");
    check(doc.begin().blockFormat().bottomMargin() == 8, "Heading bottom spacing was not applied");
    check(doc.begin().next().next().blockFormat().topMargin() == 14, "Heading spacing was not applied");
    QTextCursor cursor(&doc);
    cursor.movePosition(QTextCursor::End);
    cursor.insertText(" edited");
    const auto edited = doc.toMarkdown();
    DocumentSpacing::apply(&doc);
    doc.undo();
    DocumentSpacing::apply(&doc);
    check(doc.toMarkdown() == original && doc.isRedoAvailable(), "Spacing broke undo or discarded redo");
    doc.redo();
    check(doc.toMarkdown() == edited, "Spacing broke redo");
    cursor.movePosition(QTextCursor::End);
    cursor.beginEditBlock();
    cursor.insertBlock(QTextBlockFormat());
    cursor.insertText("New paragraph");
    cursor.endEditBlock();
    const auto added = doc.toMarkdown();
    DocumentSpacing::apply(&doc);
    doc.undo();
    check(doc.toMarkdown() == edited, "Paragraph spacing did not join its text edit");
    doc.redo();
    check(doc.toMarkdown() == added, "New paragraph redo failed");
    qInfo("PASS: paragraph spacing, Markdown, undo and redo");
}
