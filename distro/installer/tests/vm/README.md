# vmdrive: QEMU serial and QMP driver for the live ISO

`vmdrive` boots the Bingux live ISO in QEMU (KVM, q35 for UEFI or pc for BIOS), talks to the
autologin root shell over a serial unix socket, and takes screenshots over QMP. It uses only the
Python standard library and needs `qemu-system-x86_64`, `qemu-img`, and OVMF (UEFI).

## Smoke test

From this directory:

```
python3 -m vmdrive smoke --iso ../../out/bingux-os-2026.10.21-x86_64.iso --firmware uefi --out ../../out/installer-backend/driver-smoke/uefi
python3 -m vmdrive smoke --iso ../../out/bingux-os-2026.10.21-x86_64.iso --firmware bios --out ../../out/installer-backend/driver-smoke/bios
```

The smoke test boots with one 4 GiB raw disk in /tmp, waits for the shell, runs
`uname -r`, `lsblk -b -d -o NAME,SIZE` and `cat /etc/os-release | head -3`, takes a screenshot,
powers off, and writes `transcript.log`, `commands.jsonl`, `results.json` and `screen.png` to `--out`.
Exit status is 0 only if every check passes. Options: `--timeout` (shell wait, default 300 s),
`--ram-mb` (4096), `--cpus` (4). OVMF paths can be overridden with `BINGUX_OVMF_CODE` and
`BINGUX_OVMF_VARS`.

## API (`vmdrive.vm`, `vmdrive.checks`)

- `LiveVm(artifacts, scratch=None)`: one guest. Logs go to `artifacts`; sockets and disks go to a
  fresh `/tmp/vmdrive-*` directory, removed by `close()`. Works as a context manager (powers off
  and closes on exit).
- `start(iso=None, disks=(), firmware="uefi", ram_mb=4096, cpus=4, cdrom=None, extra_disks_format="qcow2", name="bingux-live")`:
  starts QEMU in the background. `iso` is attached as `-cdrom` with `-boot d`. `disks` is a list of
  `(path, size_bytes)`, created if missing and attached as virtio-blk (`vda`, `vdb`, ...). UEFI uses
  a copy of OVMF_VARS, so NVRAM is fresh each run.
- `wait_for_shell(timeout=300, boot_delay=30) -> bool`: stays silent for `boot_delay` seconds so
  the bootloader's auto-boot runs (keys typed into its menu select entries), then sends
  `echo BINGUX-READY-$((1+1))` every 3 s until `BINGUX-READY-2` appears. Returns False on timeout.
- `run(command, timeout=600, check=False, on_line=None) -> (rc, output)`: runs a command through
  the serial shell with a unique end marker. ANSI is stripped. `rc` is None on timeout.
  `on_line(line)` streams complete output lines as they arrive. `check=True` raises `CommandFailed`
  on non-zero rc.
- `send(text)`: raw text to the serial console, no newline added.
- `wait_for(needle, timeout=300) -> bool`: waits for `needle` in serial output produced after the call
  (for example `login:` or `assphrase`). Call it right after `start()` for prompts that appear during boot.
- `screenshot(path)`: QMP `screendump`, converted to PNG with the built-in encoder.
- `poweroff(timeout=120)`: ACPI power button via QMP; kills QEMU if it has not exited.
- `kill()`, `close()`, `alive()`: process and cleanup control.
- `make_disk(path, size_bytes, fmt="raw"|"qcow2")`: `qemu-img create`.
- `write_png(path, width, height, rgb)`, `read_ppm(path)`: image helpers.
- `Checks(artifacts)` (`vmdrive.checks`): `expect(condition, message, detail="")` records and prints
  a PASS/FAIL line and returns the boolean; `save()` writes `results.json`; `ok` is the overall result.

Logs in `artifacts`: `transcript.log` (serial output and `##` event lines), `commands.jsonl`
(one object per command: `cmd`, `rc`, `seconds`, `timed_out`, `output`), `qemu.log` if QEMU wrote
to stderr, and `results.json`.

## Notes

- The guest is not modified. The serial getty is requested through an SMBIOS type 11 unit
  drop-in, as in `tests/distro_vm.py`, and the ISO's own autologin drop-in gives the root shell.
- Bootloader menus are on the serial line in BIOS mode. Do not send input before the bootloader
  has auto-booted; `wait_for_shell` already waits.
