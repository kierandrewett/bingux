#pragma once

#include <QObject>
#include <QPointer>
#include <QQuickTextDocument>
#include <QTextBlock>
#include <QTextCursor>
#include <QTextDocument>
#include <QTimer>

class DocumentSpacing : public QObject {
    Q_OBJECT
    Q_PROPERTY(QQuickTextDocument* document READ document WRITE setDocument NOTIFY documentChanged)
  public:
    using QObject::QObject;
    QQuickTextDocument* document() const {
        return m_document;
    }
    void setDocument(QQuickTextDocument* document) {
        if (document == m_document)
            return;
        if (m_document)
            disconnect(m_document->textDocument(), nullptr, this, nullptr);
        m_document = document;
        if (document) {
            connect(document->textDocument(), &QTextDocument::contentsChange, this,
                    [this] { schedule(); });
            schedule();
        }
        emit documentChanged();
    }
    static void apply(QTextDocument* doc) {
        // Undo restores already-styled blocks. Do not discard its redo branch.
        if (doc->isRedoAvailable())
            return;
        QList<QPair<QTextBlock, QTextBlockFormat>> changes;
        for (auto block = doc->begin(); block.isValid(); block = block.next()) {
            auto format = block.blockFormat();
            format.setLineHeight(115, QTextBlockFormat::ProportionalHeight);
            format.setTopMargin(format.headingLevel() > 0 && block.position() > 0 ? 14 : 0);
            format.setBottomMargin(format.headingLevel() > 0 ? 8 : 0);
            if (format != block.blockFormat())
                changes.append({block, format});
        }
        if (changes.isEmpty())
            return;
        const bool modified = doc->isModified();
        const bool initial = doc->availableUndoSteps() == 0;
        const bool undoEnabled = doc->isUndoRedoEnabled();
        // Loading presentation must not create an initial Undo action.
        if (initial)
            doc->setUndoRedoEnabled(false);
        QTextCursor edit(doc);
        if (!initial)
            edit.joinPreviousEditBlock();
        for (const auto& [block, format] : changes) {
            QTextCursor cursor(block);
            cursor.setBlockFormat(format);
        }
        if (!initial)
            edit.endEditBlock();
        if (initial)
            doc->setUndoRedoEnabled(undoEnabled);
        doc->setModified(modified);
    }
  signals:
    void documentChanged();

  private:
    void schedule() {
        if (m_pending || m_applying)
            return;
        m_pending = true;
        QTimer::singleShot(0, this, [this] {
            m_pending = false;
            if (!m_document)
                return;
            m_applying = true;
            apply(m_document->textDocument());
            m_applying = false;
        });
    }
    QPointer<QQuickTextDocument> m_document;
    bool m_pending = false;
    bool m_applying = false;
};
