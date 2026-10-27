# bsm appearance: accounts, published images, and the converter

Crate: `packages/bsm-appearance` (library `bsm_appearance`, binary `bsm-imgconv`). It is
the implementation of "Accurate accounts and appearance" in
[architecture.md](architecture.md#appearance-publishing). It has no D-Bus service, no
compositor code and no QML; `bsm` (the manager) calls it from a worker thread.

Everything here is synchronous and blocking.

## What the greeter may know about an account

| Item                 | Source                                                              | Who writes it                    |
| -------------------- | ------------------------------------------------------------------- | -------------------------------- |
| account list, names  | NSS plus AccountsService ([accounts](#accounts))                    | read-only                        |
| colour scheme, accent, clock format, reduced motion, opt-out | `PublishAppearance` record ([record](#the-record)) | the account's own shell          |
| wallpaper and avatar | sanitised copies in `<cache>/<uid>/` ([cache](#the-cache))          | `bsm` only, through `bsm-imgconv` |

The greeter reads files below `/var/lib/bingux/greeter/<uid>/` and nothing else. It never
gets a path into a home directory.

## Privilege model

`bsm` runs as the unprivileged `bsm` account with no capabilities, and so does `bsm-imgconv`.
Consequences, all implemented:

* **bsm never opens a path a user gave it.** `PublishAppearance` carries the images as file
  descriptors (`ImageUpdate::Set(File)`) that the account's own process opened, so the
  kernel's permission checks of that user decide what can be read. The record has no path
  field at all.
* The converter takes descriptors only (`--source-fd`, `--out-fd`, `--blur-fd`). It has no
  `--input`, no `--root`, no `--uid`; those flags are usage errors (tested).
* For accounts that are not signed in there is no user process to hand over a descriptor.
  `bsm` itself opens two trusted system sources, with `O_NOFOLLOW` on every component below
  a fixed root: the AccountsService icon and the `bingux-session export` snapshot
  ([adoption](#adoption-of-bingux-session-export)).
* `bsm-imgconv` refuses to run as root (exit 25). There is no uid drop because there is
  nothing to drop from.

## Accounts

`accounts::list_accounts(&AccountPolicy) -> Result<Vec<Account>, AccountsError>` and
`accounts::evaluate(&AccountPolicy)` (same data with the reason for each exclusion).

Sources, in order of authority:

1. **NSS**: `getpwent` for the enumeration (serialised by a lock because it has global
   state), `getpwuid_r` for accounts AccountsService caches that NSS did not enumerate (a
   directory user who logged in before).
2. **AccountsService** over the system bus (`org.freedesktop.Accounts`, `ListCachedUsers`
   and the `org.freedesktop.Accounts.User` properties) for real name, icon file, locked,
   system-account and local-account flags. If the bus or service is missing, the error is
   recorded in `Evaluation::accounts_service_error` and AccountsService's own state files
   (`/var/lib/AccountsService/users/<name>`, keys `SystemAccount` and `Icon`) fill in.
3. `/etc/shadow`, when readable (it is not for `bsm`), for locked and expiry. Unreadable
   means "unknown", which is reported as not locked and not expired.

An account is excluded, with this `Exclusion` and this precedence:

| Reason          | Rule                                                                                                       |
| --------------- | ---------------------------------------------------------------------------------------------------------- |
| `InvalidName`   | not a plain login name (control characters, whitespace, `/`, `:`, leading `-` or `.`, over 256 bytes)     |
| `Reserved`      | uid 0, 65534 (nobody), 4294967294, 4294967295                                                              |
| `SystemUid`     | uid below `UID_MIN`, or, for a **local** account, above `UID_MAX` (from `/etc/login.defs`, 1000 and 60000 when absent). A non-local account (AccountsService `LocalAccount=false`, or a name that is not in the local `/etc/passwd`) is only required to be at or above `UID_MIN`, because directories hand out uids in their own ranges |
| `NoLoginShell`  | shell basename is `nologin`, `false`, `sync`, `halt` or `shutdown` (an empty shell is allowed)             |
| `SystemAccount` | AccountsService `SystemAccount`; when that property cannot be read the account is treated as a system one  |
| `Hidden`        | name or `uid:N` line in `/etc/bsm/hidden-users` (`#` comments), or `AccountPolicy::extra_hidden`           |
| `Disabled`      | `/etc/shadow` expiry day has passed                                                                        |

A repeated uid is listed once (first entry, like `getpwuid`). A locked password does not
exclude an account (fingerprint and smart card logins still work); `Account::locked` is
there for the caller's policy. The result is sorted by display name.

The display name is AccountsService `RealName`, else the first GECOS field, else the login
name, with control characters removed and at most 128 characters (tested with a 100,000 character GECOS). `<`, `>` and `&` are deliberately **not** altered here: escaping depends on the renderer, and the greeter shows names as plain text, so a name like `<b>Bob</b> & Co` is passed through unchanged. `Account::icon_file` is the
AccountsService `IconFile`, else the state file `Icon=`, else a plain file named after the
account in the icons directory. It is **unvalidated**; the publisher validates it.

Reused from `bingux-session/src/accounts.rs` (copied, the crates are separate): the
`UID_MIN`/`UID_MAX` parsing, the nologin shell idea and the GECOS rule.

## The record

`record::PublishRecord { appearance: Appearance, show_personal_images: bool }`, built by
`PublishRecord::new(..)` or `PublishRecord::from_json(&str)`. Whitelist, all or nothing:

```json
{"color_scheme":"dark","accent":"#3584e4","clock_format":"24h","reduce_motion":false,"show_personal_images":true}
```

* at most 2048 bytes before parsing; unknown keys, duplicate keys and wrong types are errors;
* `color_scheme` is `dark` or `light`; `clock_format` is `12h` or `24h`;
* `accent` is exactly `#rrggbb` (either case on input, lower case after): no shorthand,
  alpha, names, `rgb()` or whitespace;
* `show_personal_images` defaults to `true`.

**Contrast.** An accent with less than 3:1 WCAG contrast against the scheme's background
(`#161616` dark, `#f2f2f2` light) is moved towards white (dark) or black (light) in 5 %
steps until it has it. The function is deterministic and idempotent, so republishing the
adjusted record changes nothing. Nothing in a record is executed or placed in QML source.

The uid is not in the record. The caller passes it to `publish` from the D-Bus peer's credentials.

## The cache

```
<cache>/<uid>/avatar          PNG, square, at most 512x512
<cache>/<uid>/wallpaper       JPEG, fitted into 3840x2400, never upscaled
<cache>/<uid>/wallpaper-blur  JPEG, blurred and dimmed, at most 960x600
<cache>/<uid>/profile.json    revisions, content hashes, validated appearance
<cache>/<uid>/.lock           flock target
```

`<cache>` is `bsm_proto::paths::GREETER_CACHE_DIR` (`/var/lib/bingux/greeter`) in production.

**Directory trust.** `<cache>/<uid>` is created mode 0755 and must be a directory (never a
symlink) owned by the publisher's euid; anything else is `PublishError::UntrustedDirectory`
and nothing is written. A directory the publisher owns with a loose mode is set back to 0755.
All file operations go through the directory descriptor (`openat`, `renameat`, `unlinkat`).

**Squatted directories.** If `<cache>/<uid>` exists but is a symlink, a file, a directory
owned by someone else, or a directory the publisher cannot enter, `publish` does not stay
blocked: it builds a fresh 0755 directory under a private name, swaps it in with
`renameat2(RENAME_EXCHANGE)` (one atomic step, with a plain two-rename fallback), and moves the
squatter to the hidden name `.squatted.<uid>.<time>.<n>`; a non-directory or empty directory is
then removed, a populated one is left there for an administrator. The squatter is never
followed. This needs write access to the cache root, so it works when the root belongs to the
publisher (the recommended bsm-owned root). In a sticky root owned by someone else the rename is
refused and the account stays `UntrustedDirectory`. Read-only calls (`load_cached`) never repair
anything. Tested for a symlink, a file, an unreadable populated directory and a read-only
root; a directory owned by *another uid* needs root to create and is not tested.

*Packaging note:* the legacy `bingux-session` root is a sticky, world-writable directory from
tmpfiles. A user can pre-create `<cache>/<someone else's uid>` there; bsm then refuses to use
it (that user's images stop updating, nothing is exposed). bsm should create the directories
of all eligible accounts at startup (`Publisher::adopt_system_sources` creates and locks
them). The cleaner fix is a `bsm`-owned, mode 0755 cache root plus a separate legacy
snapshot directory; that needs a `bingux-session` change and is not done here.

**Atomic updates.** Every file is written to `.<name>.tmp.<pid>.<n>` in the same directory,
`fsync`ed, then `renameat`ed over its name; `profile.json` is replaced the same way and the
directory is `fsync`ed. A reader sees the old file or the new one. `wallpaper` and
`wallpaper-blur` are two renames, so for a moment a reader can hold the new wallpaper and the
old blur; the revision in `profile.json` (and the snapshot the manager pushes) changes only
after both are in place, so a client that reloads by revision converges. **Converter output is checked before the rename.** The parent does not decode pixels, but
`image_ops::inspect_output` checks each result file: expected container signature, JPEG `EOI` or
PNG `IEND` at the end (no truncation or trailing data), the format's own header parses and
declares exactly the reported width and height, and the size fits the role (avatar square and
at most 512, wallpaper at most 3840x2400, blur at most 960x600). A converter that lies is a
warning and the previous copy stays (tested with a fake converter). Files are created `O_EXCL`,
`fchmod`ed to 0644 regardless of umask.

A failed or killed
publish leaves no half file under the final name; leftover `.tmp.` files are removed on drop
(a crashed process can leave one, which is harmless and has an unreachable name).

**Revisions.** `avatar_rev` and `wallpaper_rev` (the wallpaper and its blur share one) are
stored in `profile.json` and are monotonic:

* they increase by exactly one when the published content changes, including when an image is
  added or removed;
* they do not change when the same pixels are published again, when only the appearance
  changes, or when the source bytes are identical (the converter is skipped by source SHA-256,
  and an identical result is not installed);
* they never go back. If `profile.json` is missing next to existing images, or damaged, the
  counters restart from the current Unix time in seconds, so a client that cached `?v=N`
  never meets the same `N` for different content. A `profile.json` with a newer `version`
  is `PublishError::UnsupportedState`, not overwritten.

**Failure keeps the last copy.** If a source is malformed, too large, a directory, refused by
the sandbox, or the converter times out, the call still succeeds, the earlier copy and its
revision stay, and `Published::warnings` carries an `ImageWarning { image, error,
kept_previous }`. This is also the answer for encrypted homes: the account's shell is not
running, nobody publishes, and the last copies (or the neutral fallback when there are none:
`avatar_path`/`wallpaper_path` are `None`) are shown. An invalid *record* is an error for the
whole call and changes nothing.

**Restart.** `Publisher::load_cached(uid)` rebuilds `Published` from `profile.json` and the
files; a file that has gone missing is not advertised.

### Adoption of `bingux-session export`

`bingux-session export` writes `<cache>/<user name>/wallpaper` (and `session.json`,
`wallpaper-blur`) as the user, from the wallpaper Bingux's own configuration resolves to. bsm
does not create a second wallpaper source; it adopts that one. The exact rule of
`Publisher::adopt_system_sources(uid, SystemSources { user_name, icon_file })`:

1. It does nothing for an account that opted out.
2. A copy that came from the account's own descriptor (`Origin::User`) is never replaced.
3. The wallpaper source is `<cache>/<user_name>/wallpaper`, opened with `O_NOFOLLOW` on every
   component below `<cache>`. It is used only if both `<user_name>` directory and the file are
   regular, not symlinks, and owned by `uid` (so one user cannot dress up another's tile). A
   user name that is empty, starts with `.`, contains `/` or NUL, or is all digits (it would
   collide with a uid directory) is refused.
4. It is never served directly. It goes through `bsm-imgconv` like any other source, and
   the sanitised result is published under `<cache>/<uid>/`. The snapshot's own
   `wallpaper-blur` is ignored and regenerated, so every account has the same blur.
5. A snapshot that disappears removes a copy that came from a snapshot, and only that.
6. The avatar source is the AccountsService `icon_file`, accepted only below one of
   `PublisherConfig::icon_roots` (default `/var/lib/AccountsService/icons`), with no symlink
   below the root, and with the file and directories owned by `trusted_icon_owners` (default
   root).

## The converter, `bsm-imgconv`

```
bsm-imgconv convert --kind avatar|wallpaper --source-fd N --out-fd N [--blur-fd N]
                    [--if-unchanged SHA256] [--max-input-bytes N] [--max-pixels N] [--max-side N]
                    [--memory-limit BYTES] [--cpu-seconds N] [--max-output-bytes N] [--require-seccomp]
```

Installed as `/usr/libexec/bsm/bsm-imgconv` (`convert::DEFAULT_PROGRAM`). The parent is
`convert::Converter::convert`, which spawns it with a cleared environment, no stdin, pipes
for stdout and stderr (read up to 64 KiB), `PR_SET_PDEATHSIG`, and a wall-clock timeout
(default 30 s, then `SIGKILL`). The parent creates the output files (fresh, `O_RDWR`, 0644)
and passes them as descriptors; the child never needs write access to a directory.

Order inside the child (the security design):

1. parse arguments; refuse root; `PR_SET_DUMPABLE 0`; rlimits `CORE=0`, `AS` (1.5 GiB),
   `CPU` (20 s), `NOFILE` (32), `FSIZE` (32 MiB, the most it can write), `NPROC=0` (best effort);
2. check that source and outputs are regular files, check the source size against the cap
   (48 MiB wallpaper, 16 MiB avatar), read the source with `pread` from offset 0 (the shared
   offset is never used) with a cap that also catches growth after `fstat`, hash it;
3. close every other descriptor (`close_range`), then:
   * `unshare(CLONE_NEWNET)`, or `CLONE_NEWUSER|CLONE_NEWNET` for an unprivileged process
     (best effort; needs unprivileged user namespaces; reported in the output),
   * `PR_SET_NO_NEW_PRIVS`,
   * a seccomp-bpf allow list (`seccompiler`, pure Rust), default `KILL_PROCESS`;
4. decode (JPEG, PNG or WebP only; the header's dimensions are checked against 64 Mpixel and
   16384 px per side before any pixel is decoded, decoder `max_alloc` is half of `RLIMIT_AS`),
   apply EXIF orientation, resize, blur, re-encode, write.

**Seccomp allow list.** `read`, `close`, `munmap`, `mremap`, `brk`, `madvise`, `exit`,
`exit_group`, `rt_sigreturn`, `rt_sigprocmask`, `sigaltstack`, `getrandom`, `clock_gettime`,
`sched_yield`, `getpid`, `gettid`; `tgkill` only when its process and thread arguments are this converter's own pid and tid (the Rust abort path; `kill`, `tkill` and `tgkill` at any other process die with `SIGSYS`); `futex` only with the process-private wait, wake, wait-bitset and wake-bitset operations; `write` only to fd 1, fd 2 and the output
descriptors; `mmap` and `mprotect` only without `PROT_EXEC`; `fcntl` only with `F_GETFD`. No
`open*`, `socket`, `connect`, `execve`, `clone`, `fork`, `ptrace`, `unshare`, `mount`,
`prctl`. A forbidden call kills the process with `SIGSYS`; `bsm-imgconv selftest <probe>`
runs the probes `socket`, `open`, `exec`, `write`, `exec-memory`, `kill-other`, `kill-raw`, `futex-shared`, `kill-self` and `futex-private` (both allowed) and `memory` after lock-down,
and the tests assert the kill.

**The gap, precisely.** After lock-down the child can still make the allowed calls, so it can
exercise the kernel's memory-management, futex, `read` and `write` code and nothing else; it
cannot open, create or send anything. The residual risks are a kernel bug in those few
paths and CPU/memory exhaustion up to the rlimits (bounded, then killed). The network
namespace is defence in depth only and may be absent; seccomp already forbids `socket`. The
filter is compiled for the build's architecture; only x86_64 has been built and run. A
`require_seccomp` converter fails closed (exit 25) if the filter cannot be installed; without
`--require-seccomp` it continues with the other layers and says so in its report
(`seccomp: false`). The production caller (`Converter::system()`) requires it.

**Images produced.** Avatar: centred square crop, at most 512x512, PNG with alpha.
Wallpaper: fitted in 3840x2400 (no upscaling), transparency flattened onto `#161616`, JPEG q90.
Blur: the wallpaper reduced to at most 960x600, Gaussian blur with
`WALLPAPER_BLUR_RADIUS` 32 px **at a 1920 px reference width** (sigma scales with the copy's
width), then dimmed by `WALLPAPER_DIM` 0.65, JPEG q88. Those are the constants and values of
`bingux-session`'s `snapshot.rs`; the copy is smaller because a sigma-32 blur has no detail
left to store, and QML scales it up. Re-encoding removes EXIF (including GPS), ICC profiles,
animation frames and trailing data.

**Exit codes** (`convert::exit`, mapped to `ConvertError`): 0 ok, 10 unchanged (source hash
equals `--if-unchanged`), 20 usage, 21 source unreadable, 22 source refused (not a regular
file, too big), 23 not a valid image, 24 over the pixel/dimension/memory limits, 25 sandbox
could not be established, 26 output unusable. Death by signal is `ConvertError::Killed`
(`SIGSYS` = sandbox violation, `SIGKILL`/`SIGXCPU` = CPU limit or timeout, `SIGABRT` = out of
memory).

## The greeter profile

`profile::build_user_profile(account, published, availability, session) -> UserProfile`
fills the `bsm_proto::control::UserProfile`; `Published::apply_to` does the image fields.

`profile::validate_user_profile(&profile, cache_dir)` must pass before a profile is sent to
the greeter:

* `avatar_path`, `wallpaper_path`, `wallpaper_blur_path` are each either `None` or **exactly**
  `<cache>/<uid>/avatar`, `<cache>/<uid>/wallpaper`, `<cache>/<uid>/wallpaper-blur` for the
  profile's own uid. The comparison is on the string, so `..`, `.`, repeated or trailing
  slashes, another uid's directory, `profile.json` and a path in a home directory all fail;
* an account that opted out (`show_personal_images == false`) has no image paths; a blur path
  needs a wallpaper path;
* `user_name` is a plain login name, `display_name` has no control characters and at most 128
  characters, `accent` is canonical lower-case `#rrggbb`.

`profile::verify_user_profile_files` adds a disk check: each named file must be reached without
a symlink, be a regular file and be owned by the publisher.

## Privacy trade-off and opt-out

The cache copies are **system-readable by design**: files are 0644 in a 0755 directory so the
greeter account, which has no access to any home directory, can show them before anyone logs
in. Consequences a user should know:

* any local account can read any other account's published avatar and wallpaper, and the
  `profile.json` of the account (colour scheme, accent, clock format, reduced motion, content
  hashes). The images are resized and re-encoded, so EXIF metadata and the original file are
  not exposed, but the picture itself is;
* they persist on disk, readable, until the account opts out, clears the image or is removed
  (`Publisher` does not delete a removed account's directory; account removal tooling should);
* a wallpaper that is private (a photo of people, a document) should not be published.

**Opt-out** is `show_personal_images = false` in the record (a Bingux setting, per account).
It deletes `avatar`, `wallpaper` and `wallpaper-blur` from the cache immediately, keeps
publishing the appearance values, bumps the revisions so clients drop the old pictures, stops
`adopt_system_sources` from filling anything for the account, and makes the profile carry no
image paths (the greeter then draws initials and the neutral wallpaper). The images come back
only with an explicit publish after the account opts in again.

## Interface summary

```rust
accounts::list_accounts(&AccountPolicy::system()) -> Result<Vec<Account>, AccountsError>
record::PublishRecord::{new, from_json, validate}
publish::Publisher::new(PublisherConfig::system())
publish::Publisher::publish(uid, &PublishRecord, Sources { avatar: ImageUpdate, wallpaper: ImageUpdate }) -> Result<Published, PublishError>
publish::Publisher::adopt_system_sources(uid, &SystemSources) -> Result<Published, PublishError>
publish::Publisher::load_cached(uid) -> Option<Published>
profile::{build_user_profile, validate_user_profile, verify_user_profile_files}
convert::Converter::{system, new}.convert(&ConvertRequest) -> Result<ConvertReport, ConvertError>
```

`ImageUpdate` is `Keep`, `Clear` or `Set(File)`. The publisher takes an exclusive `flock` per
account, so concurrent calls (several threads or processes) serialise; tested with four
threads publishing the same wallpaper (one revision).

## Verification status

Run (`cargo test` in `packages/bsm-appearance`, 29 unit + 11 + 14 + 19 integration tests,
all passing at the time of writing):

* account filtering on a fixture root (reserved, system, nologin, hidden by name and by uid,
  AccountsService system flag, expired, duplicate uid, invalid names, custom and missing
  `login.defs`, missing shadow); the real system through AccountsService on the bus (source
  reported as `AccountsService`, the current user listed);
* record rejection (bad accents, unknown enum values, unknown, duplicate and mistyped keys,
  megabyte values, contrast lift);
* the real converter: success with all layers reported, avatar square, shared offset ignored,
  unchanged source skipped by hash, malformed, truncated, text, GIF and SVG sources, a 81
  Mpixel PNG rejected from its header in well under 10 s, a 20000 px side, an oversized file,
  a 64 MiB address-space limit, directory, device and pipe sources, a used output file,
  a path argument, a hung converter killed at the timeout, a missing converter binary, and
  `SIGSYS` for `socket`, `open`, `exec`, a write to a foreign fd and executable memory;
* the publisher: first publish and file modes, revision rules, clear, opt-out, keep-last-copy
  on bad and hostile sources, bad record changes nothing, concurrent readers never see a
  partial file (two reader threads, 12 alternating publishes, every read equals one of the two
  complete versions), four concurrent publishers, planted symlink directory, loose mode,
  restart and damaged `profile.json`, icon and snapshot adoption rules including symlinks,
  foreign owner, numeric and traversal names;
* `bsm-imgconv` by hand on a real 3860x2573 wallpaper
  (`~/.local/share/backgrounds/...-unsplash.jpg`, 1,392,039 bytes): output
  `wallpaper` 3600x2400 JPEG 1,567,101 bytes, `wallpaper-blur` 900x600 JPEG 20,813 bytes,
  1.7 s and 200 MB peak RSS, report `rlimits`, `no_new_privs`, `network_namespace`, `seccomp`
  all true; the repo's `docs/images/bingux-os-desktop.webp` (7,748 bytes) as avatar gives a
  512x512 RGBA PNG of 47,486 bytes; a text file gives exit 23, a directory exit 22.

Not verified, or not possible here:

* the converter as root (refused by design; the refusal is unit tested only as "not root");
* seccomp on aarch64 and the netns fallback on kernels without unprivileged user namespaces;
* non-local (directory) accounts: the uid-range rule is implemented but only the
  AccountsService `LocalAccount` path decides it, which no test drives; the NSS "not in local
  passwd" fallback is likewise untested;
* AccountsService with several cached users, and `Locked` from the bus (the dev machine has one
  user);
* ownership checks for a *foreign* owner of the cache directory (needs root to create);
* `/etc/shadow` expiry and lock on a real system (the file is root-only; the parser is tested
  on a fixture);
* the `PublishAppearance` D-Bus method itself, its fd passing, and the `bsm` startup creation
  of per-account directories, which belong to the manager.
