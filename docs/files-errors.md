# Errors of the file operations

`bingux_files::ops::OpError` says why an operation did not happen. Until now
about a hundred of its values carried an English sentence (`OpError::Other("The
drive is read-only.".into())`), and `Display` wrote it. The Files window put that
sentence in front of a person, so the one part of the application that
Fluent could not reach was the part people read most often when something goes
wrong.

## The rule

The domain crate never writes a sentence for a person. It says **what
happened** as a value; the window maps the value to a Fluent message
(`crates/ui/src/files/op_error.rs`, messages `ui-files-error-*`).

`Display` stays, for developers only: a short English line for logs, test
failures and `eprintln!`. Nothing in `bingux-ui` or an app calls
`OpError::to_string()` to show to a person.

## The shape

```rust
pub enum OpError {
    Exists, Permission, NotFound, Cancelled, CopyNeeded,
    Invalid(NameError),
    CannotTrash(TrashRefusal),
    /// A program the operation needs is not installed: its name (`zip`,
    /// `7-Zip`, `ffmpeg`), which is not a sentence.
    Missing(String),
    /// A call to the drive failed for a reason with a name (`IoKind`).
    Io(IoKind),
    /// Everything else: a kind and the facts its message needs.
    Failed(Failure),
}
```

`Failure` (`ops/failure.rs`) names the kind and holds names, counts, an
`IoKind` or a `Step`; it never holds a sentence. The one exception is the
line a program wrote itself (`Failure::ToolFailed { step, detail }`), which the
window shows after a localised lead, as it shows a file name.
`OpError::Unsupported` and `OpError::Other` are gone.

The reasons next to it follow the same rule:

- `NameCheck` is the value; the window words it (`ui-compress-name-*`).
- `Format::unavailable()`, `lock_unavailable()`, `split_unavailable()` and the
  `Tools::*_unavailable()` functions return `Option<OpError>`.
- `Excludes::parse` returns a `PatternError` holding a `PatternProblem`.
- A file that fails its check after being made is `CheckProblem`.
- Volume sizes are numbers (`SPLIT_SIZES_MIB`); the window words them
  (`ui-compress-split-mb`, `ui-compress-split-gb`).

Tests compare values, not sentences.

## The window

`actions::error_words(&OpError) -> String` (`actions/op_error.rs`, with
`actions/failure_text.rs` for `Failure`) is the one place that turns an error
into words: messages `ui-files-error-*`, `ui-files-failure-*`,
`ui-files-step-*`, `ui-files-check-*`. The catalogue check proves every
message exists and is used.

## Still in English

- Progress labels such as "Checking the archive" (`Progress::set_current`).
- `ImageFormat::label()` and `VideoCodec::label()` (format names).
- The other domain crates (`calendar`, `weather`, `text`) follow the same rule:
  a kind and its facts in the crate, the sentence in the app's catalogue.
  `calendar` is done (`ics::Warning`, `ics::ParseError`, `alarm::Notice`).
