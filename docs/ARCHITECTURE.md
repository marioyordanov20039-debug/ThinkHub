# Architecture

## Separation

- `src/probe.rs`: read-only hardware discovery and typed snapshots. Unknown optional attributes
  become `None`/JSON `null`. Sensor failures produce diagnostic notices. No external commands.
- `src/control.rs`: typed request allowlist, read-only planning, bounds checking, ordered writes,
  concurrent value checks and readback verification. Never accepts arbitrary write paths.
- `src/main.rs`: unprivileged CLI; scans, previews, inventory, diagnostics and named authorization requests.
- `src/inventory.rs`: CPU/memory/graphics/drive/OS discovery without unique identifiers.
- `src/controllers.rs`: bounded known-systemd-unit queries and operation-specific conflict blockers.
- `src/power.rs`: optional bounded busctl transport and current/legacy power-service adapter.
- `src/devices.rs`: DRM/input discovery, PS/2 TrackPoint identity checks and brightness conversion.
- `src/desktop.rs`: optional read-only compositor adapter; generic discovery on other desktops.
- `src/battery.rs`: charging selector parsing, preset requests and energy-based aggregation.
- `src/firmware.rs`: read-only BIOS allowlist and sanitized optional fwupd query results.
- `src/firmware_update.rs`: guarded BIOS preflight and exact-version fwupd preparation through
  system polkit authorization; no firmware operations are added to the sysfs helper.
- `src/process.rs`: bounded subprocess response capture for optional tools.
- `src/settings.rs`: validated user preferences, private atomic storage and previous-version backup.
- `src/authorization.rs`: installed-helper trust checks, fixed pkexec invocation and root-owned operation lock.
- `src/bin/helper.rs`: explicit development write entry point. Fixed `/sys` root, no environment
  overrides, ThinkPad/effective-root checks, fresh conflict checks and reversible keyboard-light test.
  No shell execution or background service. The helper cannot elevate itself.
- `tests/hardware.rs`: temporary sysfs-like fixtures for older/newer and partial hardware.

`gui/` is the unprivileged ThinkHub GTK4 frontend. It reuses typed controller/request validation
from the library and communicates with the fixed installed CLI on background workers. GTK objects
stay on the main thread. See [GUI.md](GUI.md) for its polling, draft and authorization behavior.
`gui/src/appearance.rs` validates and atomically stores app-only appearance, generates named
theme colors and derives readable accent foregrounds. It is independent of hardware preferences.

Local installation makes root-owned helper/CLI copies and
installs the named `io.github.thinkpaddashboard.control` polkit action. The policy uses
`auth_admin` for active local sessions and `no` for inactive/other sessions. No cached approval
or permissive custom polkit rule is installed. The client rejects a helper with user-writable
parents, symlinks, wrong ownership or setuid/setgid bits. It queries the root-owned helper version without privilege and requires
an exact match before authorizing hardware operations. Older helpers without version discovery
are treated as incompatible, not guessed compatible.

A root-owned, mode-0700 directory and mode-0600 no-follow lock file under `/run` serialize our
helper requests. An advisory lock does not control third-party writers. The helper rechecks
known service ownership immediately before applying. Missing status is treated conservatively.

## Kernel interfaces

Discover power supplies by `type=Battery`, not a fixed count or BAT0-only assumptions.
Battery health uses full/design energy if possible, otherwise full/design charge. Never divide
energy by charge or convert amp-hours to watt-hours without a justified voltage basis.
Discover hwmon by driver name and attributes, not `hwmon4` or another machine-specific index.
Temporary hwmon/rfkill identifiers are snapshot-local, not persistent configuration IDs.

Read generic backlight, LED and platform-profile interfaces. Distinguish firmware power
profiles from CPU governors: a missing firmware profile does not mean performance features
are entirely absent. Runtime operation errors are more authoritative than attribute presence.

## Writes and failure handling

Plan requests validate live values. Applying re-plans from fresh values, checks for unexpected
changes, performs only generated allowlisted writes, and verifies readback. Charging-mode lists are normalized to their bracket-selected value.
Verification waits up to one second for two consecutive matching reads
because LED-class writes can be queued; the write is never repeated. For threshold
pairs, increase the end first when raising the start past the old end; otherwise start first.
A failure after one write reports the partially applied state. The failing write can itself
have changed firmware even if readback failed. No automatic retry or rollback is attempted.

These checks detect some competing updates but are not an atomic transaction or ownership
protocol with TLP/power-profiles-daemon/tuned/other applications. Known competing systemd services block relevant direct writes; manually started tools and
non-systemd init systems need additional discovery. A release still needs explicit service
adapters and monitoring. The code does not claim to prevent every concurrent change.

## Privacy and portability

No telemetry, account, network dependency, serial numbers or UUIDs in the scanner.
Combined status includes the power service's application IDs/reasons for active profile holds;
local reports may therefore reveal application usage and should be previewed before sharing. Diagnostic
reports still contain machine type, component models and firmware versions; the future report
sharing flow should preview their contents. No upload exists now.

The scanner uses Linux filesystems rather than package-manager commands. Distro differences
belong in packaging and optional service adapters. Local success on CachyOS is not evidence of
every kernel/model/distro combination. Architectures and glibc baselines need release testing.
