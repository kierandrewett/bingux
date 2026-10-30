# Check status-record allocations

Run from the Bingux repository root:

```sh
cargo test --locked --offline --release -j 2 \
    --manifest-path services/statusd/Cargo.toml \
    --bin bingux-statusd record_allocation -- --nocapture
```

The fixture exercises the daemon's `record_json` function with synthetic
process records. It starts no daemon, reads no system metrics, and connects to
no desktop, audio server or device. Fixture construction is outside the
allocation counter. Only the test thread's encoding allocations are counted.

`metrics_with_desktop_state_json` takes `&Metrics`. Callers can encode the same
cached sample after desktop-state changes without copying or consuming it:

```rust,ignore
let record = metrics_with_desktop_state_json(&metrics, &desktop_state, &microphone)?;
```

The daemon uses this borrowed path for its initial record, two-second samples,
desktop-state updates and microphone updates. The socket protocol, sampling
intervals and record contents are unchanged.

## Failure modes checked before the fix

- Encoding must not allocate a copy of each cached process and argument.
- Repeated encoding must preserve the cached metrics and produce identical bytes.
- Empty arguments, quotes, backslashes, newlines and UTF-8 must survive JSON
  encoding without adding a second protocol line.
- Default optional metrics must remain omitted; unknown values must remain null.
- Desktop and microphone changes must appear immediately with the same cached
  metrics, and resetting those states must restore the original record.

## Baseline and candidate

The baseline was Bingux `b3393e562e2303c3c5a87aafcc533a59ac9a4941`, with only
the test module attached. Its test adapter called
`record_json(metrics.clone(), desktop, microphone)`, matching the daemon's
owned-argument call. The candidate adapter calls
`record_json(metrics, desktop, microphone)` with a borrowed sample.

Three behavior tests passed on the baseline. The allocation-budget test failed
at 3,605 calls. All four tests pass on the candidate. Release builds used
Rust 1.95.0 and the unchanged Cargo lockfile.

| Synthetic processes | Allocation calls before | Allocation calls after | Requested bytes before | Requested bytes after |
| ------------------- | ----------------------- | ---------------------- | ---------------------- | --------------------- |
| 0                   | 11                      | 11                     | 3,796                  | 3,796                 |
| 1                   | 20                      | 12                     | 5,132                  | 4,820                 |
| 512                 | 3,605                   | 20                     | 751,984                | 591,326               |

Calls include allocation, zeroed allocation and reallocation requests.
Requested bytes sum each request's full size, including reallocations; they
are neither peak live memory nor RSS. The 512-process case avoids 3,585 calls
and 160,658 requested bytes per publication. Serializing the output still
allocates buffers. The check allows at most 64 calls per record rather than
requiring exact implementation-dependent buffer sizes.

All three baseline and candidate output files matched byte for byte. Set
`BINGUX_RECORD_EVIDENCE_DIR` to a private directory to save those synthetic JSON
records as `0.json`, `1.json` and `512.json` for comparison.

The printed timing is the median of seven batches of 64 encodes. The test
allocator remains installed, with counting disabled during timing, so these
figures include its wrapper overhead. They are diagnostics, not a CPU-saving
gate. No whole-daemon idle CPU, desktop latency, persistent memory, physical
GPU or installed-session improvement was measured.

Run the complete package checks with:

```sh
cargo test --locked --offline --release -j 2 \
    --manifest-path services/statusd/Cargo.toml
cargo fmt --manifest-path services/statusd/Cargo.toml -- --check
```
