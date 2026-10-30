#pragma once

#include <QVariantMap>
#include "document-spacing.h"

class DocumentEdit : public DocumentSpacing {
    Q_OBJECT
  public:
    using DocumentSpacing::DocumentSpacing;

    // One native edit keeps both layout and the visible cursor out of the
    // temporary state where a Markdown fragment has been removed but not replaced.
    Q_INVOKABLE QVariantMap replace(int start, int end, const QString& markdown,
                                    bool removePlaceholder) {
        if (!document())
            return {};
        auto doc = document()->textDocument();
        if (start < 0 || end < start || end >= doc->characterCount())
            return {};
        QTextCursor cursor(doc);
        cursor.beginEditBlock();
        cursor.setPosition(start);
        cursor.setPosition(end, QTextCursor::KeepAnchor);
        cursor.removeSelectedText();
        const int before = doc->characterCount();
        cursor.insertMarkdown(markdown);
        const int added = doc->characterCount() - before;
        QVariantMap result;
        if (removePlaceholder) {
            const auto plain = doc->toPlainText().mid(start, added);
            const int marker = plain.indexOf(QChar(0x200b));
            if (marker >= 0) {
                cursor.setPosition(start + marker);
                cursor.setPosition(start + marker + 1, QTextCursor::KeepAnchor);
                result.insert("font", cursor.charFormat().font());
                cursor.removeSelectedText();
            }
        }
        result.insert("cursor", cursor.position());
        apply(doc);
        cursor.endEditBlock();
        return result;
    }
};
