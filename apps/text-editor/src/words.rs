//! The words for what `bingux-text` names as kinds: the indentation, a line
//! ending, a group of encodings and the byte order mark. The crate writes no
//! sentence; each kind is a message of this app's catalogue.

use bingux_text::encoding::{FileFormat, Script};
use bingux_text::{Indent, LineEnding};
use bingux_ui::prelude::tr;

/// `Tabs`, `Spaces: 4`.
pub fn indent(indent: Indent) -> String {
    match indent {
        Indent::Tabs => tr!("text-editor-indent-tabs"),
        Indent::Spaces(width) => tr!("text-editor-indent-spaces", width = width),
    }
}

/// What a line ending is called in a menu: the systems that use it.
pub fn line_ending(ending: LineEnding) -> String {
    match ending {
        LineEnding::Lf => tr!("text-editor-ending-lf"),
        LineEnding::CrLf => tr!("text-editor-ending-crlf"),
        LineEnding::Cr => tr!("text-editor-ending-cr"),
    }
}

/// The name of a group of encodings.
pub fn script(script: Script) -> String {
    match script {
        Script::Unicode => tr!("text-editor-script-unicode"),
        Script::Western => tr!("text-editor-script-western"),
        Script::CentralEuropean => tr!("text-editor-script-central-european"),
        Script::Baltic => tr!("text-editor-script-baltic"),
        Script::Cyrillic => tr!("text-editor-script-cyrillic"),
        Script::Greek => tr!("text-editor-script-greek"),
        Script::Turkish => tr!("text-editor-script-turkish"),
        Script::Hebrew => tr!("text-editor-script-hebrew"),
        Script::Arabic => tr!("text-editor-script-arabic"),
        Script::Vietnamese => tr!("text-editor-script-vietnamese"),
        Script::Thai => tr!("text-editor-script-thai"),
        Script::Chinese => tr!("text-editor-script-chinese"),
        Script::Japanese => tr!("text-editor-script-japanese"),
        Script::Korean => tr!("text-editor-script-korean"),
        Script::Other => tr!("text-editor-script-other"),
    }
}

/// The status bar's name for the encoding of `format`: `UTF-8`, or `UTF-8 with
/// BOM` for a file that has a byte order mark.
pub fn encoding(format: &FileFormat) -> String {
    let name = format.encoding_label();
    if format.shows_bom() {
        tr!("text-editor-encoding-with-bom", encoding = name)
    } else {
        name
    }
}
