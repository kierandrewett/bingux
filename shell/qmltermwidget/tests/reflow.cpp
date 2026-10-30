#include "Screen.h"
#include <QCoreApplication>
#include <iostream>
#include <stdexcept>
using namespace Konsole;

static void expect(bool condition, const char* message) {
    if (!condition)
        throw std::runtime_error(message);
}
static void write(Screen& screen, const std::u32string& text) {
    for (auto c : text)
        screen.displayCharacter(c);
}
static QVector<Character> image(Screen& screen) {
    int rows = screen.getHistLines() + screen.getLines();
    QVector<Character> cells(rows * screen.getColumns());
    screen.getImage(cells.data(), cells.size(), 0, rows - 1);
    return cells;
}
static QString row(Screen& screen, int index) {
    const auto cells = image(screen);
    QString result;
    for (int x = 0; x < screen.getColumns(); ++x) {
        auto c = cells[index * screen.getColumns() + x].character;
        if (c)
            result += QChar(c);
    }
    return result.trimmed();
}
static QString text(Screen& screen) {
    const int count = screen.getHistLines() + screen.getLines();
    const auto properties = screen.getLineProperties(0, count - 1);
    QString result;
    for (int i = 0; i < count; ++i) {
        result += row(screen, i);
        if (!(properties[i] & LINE_WRAPPED))
            result += '\n';
    }
    return result.trimmed();
}
int main(int argc, char** argv) {
    QCoreApplication app(argc, argv);
    try {
        Screen screen(8, 12);
        write(screen, U"ABCDEFGHIJKLMNOPQ");
        screen.resizeImage(8, 6);
        expect(row(screen, 0) == "ABCDEF" && row(screen, 1) == "GHIJKL" &&
                   row(screen, 2) == "MNOPQ",
               "shrinking must rewrap existing output");
        write(screen, U"R");
        screen.resizeImage(8, 20);
        expect(row(screen, 0) == "ABCDEFGHIJKLMNOPQR",
               "growing must join soft wraps and preserve cursor writes");
        screen.nextLine();
        write(screen, U"separate");
        for (int width : {7, 30, 4, 12, 20})
            screen.resizeImage(8, width);
        expect(text(screen) == "ABCDEFGHIJKLMNOPQR\nseparate",
               "hard line breaks must survive repeated resizing");

        Screen history(2, 12);
        history.setScroll(HistoryTypeBuffer(100));
        write(history, U"abcdefghijklmnopqrst");
        history.nextLine();
        write(history, U"NEXT");
        const auto original = text(history);
        for (int width : {5, 20, 8, 30, 6, 12}) {
            history.resizeImage(2, width);
            expect(text(history) == original,
                   "history and visible output must reflow together without loss");
        }
        for (auto* selected : {&screen, &history}) {
            const auto before = image(*selected);
            selected->setSelectionStart(0, 0, false);
            selected->setSelectionEnd(2, 0);
            const auto during = image(*selected);
            expect(during[0].rendition & RE_SELECTED,
                   "visible and historical selection must carry a display marker");
            expect(!(during[3].rendition & RE_SELECTED),
                   "selection tint must stop at its boundary");
            expect(during[0].foregroundColor == before[0].backgroundColor &&
                       during[0].backgroundColor == before[0].foregroundColor,
                   "renderer must be able to recover the original selected text colours");
            selected->clearSelection();
            expect(image(*selected)[0] == before[0],
                   "clearing selection must restore the original cell");
        }
        Screen unicode(8, 12);
        write(unicode, U"1234界x");
        unicode.resizeImage(8, 5);
        expect(row(unicode, 0) == "1234" && row(unicode, 1) == QString::fromUtf8("界x"),
               "double-width glyph must not split across rows");
        unicode.resizeImage(8, 12);
        expect(row(unicode, 0) == QString::fromUtf8("1234界x"),
               "double-width glyph must survive widening");

        Screen styled(8, 12);
        styled.setForeColor(COLOR_SPACE_SYSTEM, 1);
        styled.setRendition(RE_BOLD);
        write(styled, U"e\u0301-coloured-output");
        const auto firstGlyph = image(styled)[0];
        for (int width : {5, 18, 3, 30}) {
            styled.resizeImage(8, width);
            expect(image(styled)[0] == firstGlyph,
                   "colours, bold and combining glyphs must survive reflow");
        }
        Screen saved(8, 12);
        write(saved, U"123");
        saved.saveCursor();
        write(saved, U"456");
        saved.resizeImage(8, 2);
        saved.restoreCursor();
        write(saved, U"!");
        saved.resizeImage(8, 12);
        expect(row(saved, 0) == "123!56", "saved cursor must track reflowed text");

        Screen alternate(8, 12);
        alternate.setReflowEnabled(false);
        write(alternate, U"ABCDEFGHIJKLMNOPQ");
        alternate.resizeImage(8, 6);
        expect(row(alternate, 1) == "MNOPQ",
               "alternate-screen rows must remain application controlled");
        std::cout << "PASS: shrink/grow, cursor, hard breaks, history, Unicode, alternate screen\n";
    } catch (const std::exception& error) {
        std::cerr << "FAIL: " << error.what() << '\n';
        return 1;
    }
}
