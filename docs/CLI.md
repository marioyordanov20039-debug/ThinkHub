# ThinkHub backend reference

The public app is ThinkHub; its internal CLI retains the `thinkpad-dashboard` name.
For normal installation use [INSTALL.md](INSTALL.md).

## What works today

- Native ThinkHub window with live information, explicit change previews and controller conflict messages.
- Customize tab with dark/light themes, eight accent presets, custom colors and remembered appearance.
- BIOS update readiness, catalog refresh, risk acknowledgement and system-password authorization
  for supported fwupd BIOS releases. See [BIOS update workflow and testing limits](BIOS_UPDATES.md).

- Detect ThinkPad model, machine type, BIOS version, and the ThinkPad kernel driver.
- Read single or multiple batteries: charge, health, cycles when exposed, energy/charge
  capacities, power draw, voltage, charging status, and charge thresholds.
- Handle both energy-based and charge-based battery interfaces without mixing their units.
- Discover temperature and fan sensors dynamically instead of hardcoding `hwmon` numbers.
- Discover display and keyboard lighting, firmware power profiles, CPU policies and radio state.
- Preview validated changes to battery thresholds, lighting, firmware profiles and fan auto mode.
- Root-owned helper for explicit writes, with polkit authorization, operation locking and readback checks.
- Known competing systemd controller detection; overlapping direct writes are blocked.
- Additional CPU, RAM, graphics, drive and operating-system inventory.
- Power-profile discovery/switching through a running compatible service, including its active holds.
- Atomic user preference saves, previous-version backup and hardware change previews.
- Combined status reports and bounded JSON monitoring for the GUI.
- Charging-mode requests, charging preset previews and capacity-weighted dual-battery totals.
- Read-only BIOS information and sanitized fwupd inventory/cached-update listings.
- Display connector/resolution and input-device discovery, with read-only Hyprland output/scale information.
- Verified PS/2 pointing-stick identification, bounded TrackPoint parameter requests and reversible testing.
- App/helper version matching before requesting elevated hardware operations.
- JSON snapshots for the GUI and compatibility reports. Unique ID fields are omitted.

An interface marked **detected** still requires hardware write verification. Firmware can reject
an operation even when its Linux attribute exists. Missing data is `null`, never an invented reading.
Tests use temporary simulated hardware; they do not validate every ThinkPad model.

## Build and run

Requires Linux and Rust 1.85 or newer, plus a C linker/toolchain. Tests, Clippy and formatting
checks passed locally with Rust 1.85.0 and Rust 1.99.0. Release binaries were built with Rust 1.99.

Typical build prerequisites (install Rust from the official Rust installation instructions):

| Distro family | C toolchain prerequisites |
| --- | --- |
| Ubuntu / Debian | `sudo apt install build-essential` |
| Fedora | `sudo dnf install gcc` |
| Arch / CachyOS | `sudo pacman -S --needed base-devel` |

```sh
cargo build --release --locked
./target/release/thinkpad-dashboard scan
./target/release/thinkpad-dashboard scan --json
./target/release/thinkpad-dashboard capabilities
./target/release/thinkpad-dashboard overview
./target/release/thinkpad-dashboard doctor
./target/release/thinkpad-dashboard power show
./target/release/thinkpad-dashboard status
./target/release/thinkpad-dashboard monitor 3
./target/release/thinkpad-dashboard battery presets
./target/release/thinkpad-dashboard firmware bios
./target/release/thinkpad-dashboard devices
./target/release/thinkpad-dashboard display brightness-plan intel_backlight 50
```

The scanner runs as a normal user. It does not require Python, a desktop environment,
a web server, a Lenovo-specific package, or an internet connection at runtime.
Power-service integration currently uses the optional `busctl` transport; it requires a running
power-profiles-daemon-compatible system service and system D-Bus. The hardware scanner works
without those components.

Preview a change without applying it:

```sh
./target/release/thinkpad-dashboard plan \
  '{"kind":"battery_thresholds","battery":"BAT0","start":75,"end":80}'

./target/release/thinkpad-dashboard plan \
  '{"kind":"brightness","device":"tpacpi::kbd_backlight","value":1}'

./target/release/thinkpad-dashboard plan '{"kind":"platform_profile","profile":"balanced"}'
./target/release/thinkpad-dashboard plan '{"kind":"fan_auto"}'
```

IDs and available ranges come from discovery. `BAT0`, keyboard lighting and platform profiles
are examples, not universal requirements.

## Local installation and authorized controls

After building and reviewing the code, install the development integration with:

```sh
pkexec ./scripts/install-local.sh
thinkpad-dashboard doctor
```

This creates root-owned copies of the CLI at `/usr/local/bin/thinkpad-dashboard` and the helper
at `/usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper`, plus a polkit action. It does not
start a background daemon. For packaging tests, `scripts/install-local.sh --destdir /absolute/staging`
installs into a temporary tree without needing root.

Apply a reviewed request:

```sh
thinkpad-dashboard apply \
  '{"kind":"brightness","device":"tpacpi::kbd_backlight","value":1}'
```

The ordinary app validates the installed helper's ownership, permissions and matching app version, then requests
administrator authentication through pkexec. The named policy requires administrator
credentials in an active local session; it grants no permission to inactive/remote sessions
and does not retain authentication. A desktop polkit agent is required for the GUI prompt.
The GUI itself will never run as root.

`thinkpad-dashboard test-keyboard` requests authentication, changes the ThinkPad keyboard
backlight by one step, verifies it, and restores its original value. Readback allows for asynchronous
LED updates. Restoration is attempted
even if the test write fails. Like any one-shot hardware change, a forced process kill or
power loss can interrupt restoration. This test does not touch battery or cooling settings.

The helper accepts only named validated operations, uses a fixed `/sys` root and serializes
our operations with a protected lock under `/run/thinkpad-dashboard`. There is no arbitrary
path, shell command, force-override flag or user-supplied fixture root. It is not setuid.

Known systemd services such as TLP, tuned, power-profiles-daemon, thinkfan and unofficial
Vantage are checked for relevant conflicts. Unknown service status blocks affected direct
writes. Other software is never stopped automatically. This is conservative conflict
avoidance, not complete coordination: manually launched controllers and other init systems
still need additional detection and adapters.

Threshold changes are ordered so intermediate values remain valid. Multi-attribute writes are
not atomic: failures identify verified earlier writes and require a fresh scan. They are not
silently retried or rolled back. No fan-stop or manual-speed request is implemented. Automatic
startup application and further service integrations remain in development.

For development, `pkexec ./scripts/install-local.sh --verify-keyboard` installs and runs that
reversible test in one authorized operation. The flag is never used by default or staging tests.

Remove only this local integration with `pkexec ./scripts/uninstall-local.sh`. Source and user
reports are kept. Native distro packages use their package manager for install/remove.

## Power profiles and saved preferences

```sh
thinkpad-dashboard power show
thinkpad-dashboard power set balanced
thinkpad-dashboard power test
thinkpad-dashboard settings show
thinkpad-dashboard settings plan
```

Power profiles are distinct from firmware profiles: a laptop without the firmware sysfs
interface may still support Power Saver, Balanced and Performance through its system service.
The adapter supports current and legacy power-profiles-daemon API names. It does not start,
replace or stop a service, and does not directly manipulate CPU governors. An existing profile
hold blocks changes instead of silently cancelling another application's hold. Authorization
uses the power service's own policy, not our root helper.

`power test` temporarily selects another advertised profile and restores the original. It
attempts restoration even after a failed request/readback. A service failure, new hold, forced
process kill or power loss can prevent restoration; inspect `power show` if the command fails.
Service state is re-read after changes. Concurrent desktop actions can still change it later.

Save a desired configuration (example only; choose values for your machine):

```sh
thinkpad-dashboard settings save \
  '{"schema_version":1,"power_profile":"balanced","battery_thresholds":{"BAT0":{"start":75,"end":80}}}'
```

Preferences are stored at `$XDG_CONFIG_HOME/thinkpad-dashboard/preferences.json`, falling back
to `~/.config/thinkpad-dashboard/preferences.json`. Saves use private files, locking and atomic
replacement; `preferences.previous.json` retains the previous valid configuration. Malformed
files or a newer schema are preserved instead of overwritten. Missing settings return empty
preferences without creating a directory.

**Saving does not apply hardware settings.** There is no automatic startup/resume application.
`settings plan` shows current compatibility and blockers before individual explicit controls
are applied. Saving a desired value is not evidence the hardware supports it.

`status` combines hardware, inventory, power-service state, known controllers and preferences.
`monitor <n>` prints 1–60 JSON status samples roughly one second apart; it creates no history file
and does not change settings. See [power and preferences design](POWER_AND_PREFERENCES.md).

## Battery and firmware information

```sh
thinkpad-dashboard battery presets
thinkpad-dashboard battery preset everyday BAT0
thinkpad-dashboard plan '{"kind":"battery_charge_mode","battery":"BAT0","mode":"inhibit-charge"}'
thinkpad-dashboard firmware bios
thinkpad-dashboard firmware bios --authorized
thinkpad-dashboard firmware scan
```

Charging presets are examples, not Lenovo recommendations. The preset command only previews
thresholds and ownership blockers. Choose a preset, inspect the plan, and explicitly apply its
request if appropriate. Firmware can reject percentages despite a valid Linux interface.

Charging-mode requests support `auto` (normal charging that respects thresholds) and
`inhibit-charge` (pause charging while AC is connected), only when advertised by that battery.
They use the same authorization, conflict and readback checks as threshold changes. Forced
discharge and automatic calibration are not exposed. Modes are not saved for automatic replay.

The combined battery percentage uses energy capacities, never a simple average. Peripheral
batteries with `scope=Device` are excluded from laptop totals. Missing energy data leaves the
combined result unknown. Discharge-time estimates require complete energy/power data and
known discharging/idle battery states. Driver-reported state of health is kept separate from
full/design capacity health. A zero cycle count is treated as unavailable per the kernel ABI.

BIOS information uses a fixed list of non-password settings. Protected values remain `null`
in the normal view. The `--authorized` view uses the root helper to read the same allowlist;
it cannot edit BIOS values. A development install can verify that path with
`pkexec ./scripts/install-local.sh --verify-bios`.

`firmware scan` explicitly queries the optional fwupd client, which may start the existing
fwupd service to inspect devices. It requests no metadata refresh, download, firmware install,
reboot, security fix or BIOS change. Output contains only selected descriptive fields; serial,
GUID, device-ID and instance-ID fields are dropped. Update information is based on local
metadata of **unknown freshness**, so an empty result does not establish that firmware is
current. Ordinary status/monitor commands never query fwupd for updates.

See [battery and firmware design](BATTERY_AND_FIRMWARE.md).

## Development checks

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

See [the development plan](PLAN.md), [architecture](ARCHITECTURE.md), and
[compatibility record](COMPATIBILITY.md). Linux kernel reference interfaces:
[ThinkPad ACPI](https://docs.kernel.org/admin-guide/laptops/thinkpad-acpi.html),
[power supply](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-class-power),
[platform profiles](https://docs.kernel.org/userspace-api/sysfs-platform_profile.html).

## Display and input information

```sh
thinkpad-dashboard devices
thinkpad-dashboard display brightness-plan intel_backlight 50
thinkpad-dashboard plan '{"kind":"track_point","controller":"serio2","setting":"sensitivity","value":200}'
thinkpad-dashboard test-trackpoint
```

`devices` discovers DRM connectors and input devices independently of a desktop environment.
Repeated resolutions are deduplicated; sysfs modes are not claimed to be current/native modes
or refresh rates. The optional Hyprland read-only adapter supplies current dimensions, refresh
rate and scale. Other desktops keep generic discovery and report session details as unknown.
GNOME/KDE acceleration, touchpad preferences and display-mode controls are not implemented yet.

Input classification uses udev input flags, then kernel properties/capabilities, with name hints
marked separately. Mouse/event aliases are not duplicated. The app does not open input event
streams or grab devices. Raw unique identifier fields are not returned.

PS/2 TrackPoint sensitivity/speed requests require the psmouse driver, the kernel pointing-stick
property, the relevant attribute and an explicit controller ID from discovery. Values are limited
to 1–255; USB/I2C pointing sticks need separate control adapters. These hardware parameters are
not desktop pointer acceleration, and kernel readback does not measure physical pointer behavior.
Hotplug or other software may reset them; they are not automatically saved/replayed.

`test-trackpoint` requests authorization, changes sensitivity by one step and attempts restoration,
including after a failed test write/readback. A crash/forced kill can interrupt restoration. It
never captures input events. The development installer can combine installation and that test
using `pkexec ./scripts/install-local.sh --verify-trackpoint`.

Percentage brightness is converted to the discovered hardware range with rounding, and the
command only previews a normal brightness request. It does not alter resolution or scale.

See [input/display design](INPUT_AND_DISPLAY.md) and [backend readiness](READINESS.md).

## Experimental portable backend build

The CLI and helper can also be built as static x86_64 Linux binaries, without a glibc dependency:

```sh
rustup toolchain install 1.85.0 --profile minimal
rustup target add --toolchain 1.85.0 x86_64-unknown-linux-musl
python3 scripts/build-portable.py
```

This developer script requires Python 3 and `readelf` from binutils. It uses Rust's bundled
linker and the baseline x86-64 CPU target. It verifies that the resulting ELF files have no
interpreter or shared-library dependencies. Output is under
`target/portable/x86_64-unknown-linux-musl/release/`. These backend binaries are not AppImages.
They are not a graphical release package, and the local installer currently installs the normal
native build from `target/release/`. Optional system integrations still require the distro's
polkit, busctl/power service or fwupd components. Removing the glibc dependency does not establish
hardware, security-policy or runtime compatibility on every distro. ARM/32-bit builds are separate work.
