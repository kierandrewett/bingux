# logind, polkit, portals, audio and systemd --user for seatless hosted sessions

What the 2026.10.27 image (Arch, systemd 262, kernel 7.2.11, QEMU 10.1.5 with virtio-gpu and
two heads) does for the sessions that bsm hosts, as observed in a VM. Spec section 6 asks for
evidence, not claims, so every row says how it was observed:

* **harness**: asserted by `tests/bsm-qemu-security.py` and its result file is in
  `distro/out/bsm-test/security/` (not tracked, `distro/out/` is ignored);
* **by hand**: run once in a VM with the commands shown, no automated assertion yet;
* **blocked**: cannot be tested in this VM, with the reason;
* **not run**: written in the harness (scenario 10), no completed run.

The hosted session is a logind session of class `user`, type `wayland`, created by the PAM
service `bsm-login`, with no seat (decision D4 in [architecture.md](architecture.md)).

## What works

| Area | Observation | How |
| ---- | ----------- | --- |
| Session record | `loginctl list-sessions` shows two seatless sessions per signed-in account: class `user` (the desktop) and class `manager` (the user manager). Earlier runs recorded `Type=wayland`, `Class=user`, `Service=bsm-login`, `Remote=no`, `Active=yes`, `State=active`, `Seat=` empty, `VTNr=0`, `Desktop=gnoblin` ([packaging.md](packaging.md)). | by hand; scenario 10 asserts it, not run |
| Scope | `session-N.scope` holds the helper's supervisor `bsm-session`, `gnoblin --hosted`, the bridge `bsm-host`, Mutter (`gnoblin --wayland --headless`), Gnoblin's session supervisor and Lua worker, and the Bingux shell. Services started by `systemd --user` (PipeWire, WirePlumber, `xdg-desktop-portal*`, gvfs, at-spi, gnome-keyring, `bingux-statusd`) live in `user@UID.service`, outside the scope. | by hand, `systemd-cgls` |
| bsm's caller test | A process counts as "in the session" when its cgroup is inside `session-N.scope`. Processes of the same account started through `systemd --user` are not, so `bsmctl lock` from such a process is `unauthorised`. | harness (scenario 1), by hand |
| systemd --user | `user@UID.service` runs, the user D-Bus (`/run/user/UID/bus`, mode 0666 inside a 0700 directory) and the portals start. | by hand |
| Audio server | PipeWire, WirePlumber and pipewire-pulse run per hosted session; `pactl info` answers ("PulseAudio (on PipeWire 1.6.10)"); the nodes `gnoblin` (Stream/Output/Video) and `bsm-host` (Stream/Input/Video) exist, two each for two outputs. Another account cannot connect to the socket: the runtime directory is 0700. | by hand, `pactl`, `pw-cli ls Node` |
| Sleep inhibitor | bsm holds a `sleep` delay inhibitor ("Protect sessions before sleep"), next to NetworkManager's and UPower's. | by hand, `systemd-inhibit --list` |
| Suspend | ACPI S3 under SeaBIOS: `PM: suspend entry (deep)` ... `Waking up from system sleep state S3` ... `PM: suspend exit`, the guest answers after `system_wakeup`. | by hand, `dmesg` |
| Raw input | `/dev/input/event*` are `root:input 0660`, `/dev/uinput` is root only. An ordinary account (no `input` group) can open neither. | harness (scenario 7), by hand |
| Display node | `/dev/dri/card1` is `root:video 0660`. An account in `video` can open it, cannot become DRM master (`EACCES`) and gets handle 0 from `GETFB` for both scanout framebuffers, so it cannot read the screen. Root gets real handles (`CAP_SYS_ADMIN`). | by hand with `bsm_guest_probe.py drm`; scenario 7 asserts it, not run |
| Own session capture | `org.gnome.Mutter.ScreenCast.CreateSession` and `RecordMonitor`, and `org.gnome.Mutter.RemoteDesktop` key injection, work on the account's own session bus; a key injected that way reaches the marker window of that session. | by hand with `bsm_guest_probe.py screencast` and `rd-inject` |

## What is restricted

| Area | Observation | How |
| ---- | ----------- | --- |
| polkit and "local" | `pkcheck` treats a seatless session as not local ([helper.md](helper.md)); `50-bsm.rules` therefore names the actions a hosted session may do. `login1.reboot`, `reboot-multiple-sessions`, `power-off` are `yes` for a hosted Gnoblin; `login1.halt` is `auth_admin_keep`, `udisks2.filesystem-mount-system` is `auth_admin` (BsmPackaging, same image). | by hand |
| polkit defaults of this image | `pkaction` reports `implicit active: yes` for `login1.reboot-multiple-sessions`, `power-off-multiple-sessions` and `suspend-multiple-sessions` as well, so the rule file does not widen the stock default for an active session here (the manager review assumed the stock default needed an administrator). | by hand, `pkaction --verbose` |
| Hosted power requests | `bsm` asks polkit about the caller's bus name. A one-shot `bsmctl reboot --json` from a hosted session exits before polkit answers: polkit returns `NameHasNoOwner`, bsm fails closed and the operation ends `unauthorised`. The shell's `BsmSession.qml` runs exactly that form, so Restart and Shut Down from a hosted desktop cannot succeed. `--wait` was not verified. | by hand, journal line `polkit check failed: logind_failure: CheckAuthorization: ... NameHasNoOwner` |
| Accounts that read raw input | The image puts the `live` account in `input` and `wheel`. Any process of such an account can read the keyboard events of the machine, including passwords typed at the greeter. Ordinary accounts (the harness uses `alice` and `bob`, groups `audio` and `video` only) cannot. | by hand |
| Greeter cache | `/var/lib/bingux/greeter/<uid>/` holds re-encoded copies, world readable by design. `profile.json` also holds `source_sha256` of the original image, a hash of a private file that any account can read. | by hand and harness (scenario 1) |
| Legacy exporter | `bingux-greeter-export.service` (the greetd-era wallpaper snapshot) fails with `Permission denied` under bsm, because the cache root is now owned by `bsm`, and systemd restarts it every 3 seconds in every hosted session (221 restarts in about 12 minutes on one account). | by hand, journal |

## What is blocked

| Area | Why |
| ---- | --- |
| Hardware audio and seat ACLs (`uaccess`) | The VM has no sound card (`/dev/snd` holds only `seq` and `timer`), so PipeWire runs on its `auto_null` sink. Scenario 10 expects an ALSA sink for an account in `audio` once the VM has a card; it is not run. |
| Camera, USB storage | No device in the VM. |
| DPMS off | `SceneCommand::SetDpms` is only ever sent with `on: true` (at resume). Nothing in the manager or compositor issues DPMS off, and no other process can blank a connector while bsm is DRM master, so there is nothing to test beyond "DPMS stays on". |
| S3 under UEFI | With the host's edk2 OVMF (20260812) the guest never resumes: after `system_wakeup` the vCPU sits at `RIP=0x833f61` (a `pause; jmp` loop in the PEI phase, same address with one and with four vCPUs). `s2idle` cannot be woken in this VM either (the RTC alarm and a key press did nothing). The suspend scenario therefore uses a second VM booted through SeaBIOS and the syslinux menu (`b` selects the bsm entry). |
| bsm across a resume | In the SeaBIOS VM a bsm started after an S3 resume never became ready (`ERROR not ready after 120 s`, the unit stayed `activating`); a bsm that was already running was not tested through a resume. Cause not investigated. |
| Reboot, power off and logind refusals | Scenario 8 (denials, inhibitor, cancel, logind refusal through a polkit deny rule for `bsm`, hosted-session reboot, root reboot with the QEMU `RESET` event, greeter policy) is written, not run. |

## Environment notes that affect every run

* The host was at a load average of 55 to 70 while these runs were made. The first Gnoblin start of a fresh VM then takes 20 seconds or more; Gnoblin's runtime supervisor gives up on the private runtime handshake (`compositor did not complete the private runtime handshake`), restarts the Lua worker and falls back to the built-in configuration, Bingux never launches, and bsm ends the login at `BinguxLaunched` after 120 seconds ("Bingux could not be started"). A second attempt on the warm VM succeeds in about 40 seconds. The harness retries a login up to three times and says so.
* `pam_faillock` counts an aborted or timed-out PAM conversation as a failed login (scenario 4, both checks fail): after three account switches, or three idle minutes at the lock screen, the account is locked out although no wrong password was typed. The harness resets the counters before each password it types.
