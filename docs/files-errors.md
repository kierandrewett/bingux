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
    // unchanged, already data
    Exists, Permission, NotFound, Cancelled, CopyNeeded,
    Invalid(NameError),
    CannotTrash(TrashRefusal),

    /// A program the operation needs is not installed: its name (`zip`,
    /// `7-Zip`, `ffmpeg`), which is not a sentence. Built.
    Missing(String),
    /// A call to the drive failed for a reason with a name: `IoKind::Full`,
    /// `ReadOnly`, `QuotaFull`, `InUse`, `BetweenDrives`, ... Built.
    Io(IoKind),
    /// Still worded in English for the log, to be replaced by kinds as the
    /// table below says: `Unsupported(String)` and `Other(String)`.
    Unsupported(String),
    Other(String),
}
```

`Unsupported` and `Failure` are `#[non_exhaustive]` enums whose variants name a
kind and hold the facts a message needs (a path, a program name, an exit
status), never a sentence:

| Today                                                   | Becomes                                   |
| ------------------------------------------------------- | ----------------------------------------- |
| `Other("There is no space left on the drive.")`         | `Failed(Failure::NoSpace)`                |
| `Other("The drive is read-only.")`                      | `Failed(Failure::ReadOnly)`               |
| `Other("The folder is not empty.")`                     | `Failed(Failure::NotEmpty)`               |
| `Other("The file or folder is in use.")`                | `Failed(Failure::InUse)`                  |
| `Other("Your storage quota is full.")`                  | `Failed(Failure::QuotaFull)`              |
| `Unsupported("… between two drives in one step.")`      | `Unsupported(Unsupported::BetweenDrives)` |
| `Unsupported("{name} is not installed.")`               | `Missing(Tool::…)`                        |
| `Other("The paths must be absolute.")` and similar      | `Failed(Failure::Invalid(Why::…))`        |
| `Other(format!("{what}: {line}."))` from a program      | `Failed(Failure::Tool { what, detail })`  |
| `Other(plain(&io_error))` for an OS error we do not map | `Failed(Failure::System(String))`         |

`detail` and `System` hold text that a program or the OS wrote; the window shows
it after the localised lead (`ui-files-error-tool = { $what } failed: { $detail }`),
as it shows a file name.

The same split applies to the reasons next to it:

- `NameCheck::message()` becomes the `NameCheck` value itself; the Compress
  dialog already maps it.
- `Format::unavailable()`, `Tools::*_unavailable()` return `Option<Tool>` or
  `Option<Unavailable>` (a codec, a format), not `Option<String>`.
- `Excludes::parse` returns a typed error.

## Callers that match on text

Nobody outside the crate matches on the sentence. Inside it, tests and three
modules do:

| Where                                             | Change                                                 |
| ------------------------------------------------- | ------------------------------------------------------ |
| `ops/durable.rs` (`let OpError::Other(text) = …`) | match `Failure::Device { .. }` / `Failure::System(_)`  |
| `ops/restore.rs` (five tests and `failure.error`) | match the `Failure` kind (`RestoreTo`, `NoHome`, …)    |
| `ops/tool.rs`, `ops/archive.rs` tests             | `assert_eq!(error, OpError::Missing(Tool::Named(..)))` |

A test that compared a sentence compares a value instead; that is stricter, and
no longer breaks when the wording changes.

## The window

`op_error::text(&OpError) -> String` is the one place that turns an error into
words. `error_text(what, &error)` of the Files actions keeps its shape
(`ui-files-transfer-error`, `$what` and `$error`), so every caller changes by
passing `op_error::text(&error)` instead of `error.to_string()`.
`crate_catalogue(..).assert_clean()` proves every kind has a message.

## Order of work

1. This note.
2. `OpError` and its constructors in `ops/mod.rs`, the io mapping, and the
   window's `op_error.rs`, with a test per kind.
3. `pack`, `pack_plan`, `media_plan`, `shrink`, `tool`: tools and reasons.
4. `transfer`, `restore`, `durable`, `trash*`.
5. The other domain crates (`calendar`, `weather`, `text`) by the same rule:
   a kind and its facts in the crate, the sentence in the window's catalogue.
