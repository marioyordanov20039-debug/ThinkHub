# Power services and preferences

## Power-service adapter

`power.rs` uses the running power-profiles-daemon-compatible service as the owner of performance
policy. It never writes CPU governors or EPP behind the service's back. This also makes profile
selection useful on machines without `/sys/firmware/acpi/platform_profile`.

The current and legacy API names/object paths are supported:

- `org.freedesktop.UPower.PowerProfiles` at `/org/freedesktop/UPower/PowerProfiles`
- `net.hadess.PowerProfiles` at `/net/hadess/PowerProfiles`

The adapter resolves the well-known service name to a unique D-Bus owner and uses that owner
for subsequent reads/writes. A restarted service causes an error instead of silently directing
a pending request to a different process. A new command discovers the current owner again.

The current transport is the optional `busctl` program with JSON responses. Arguments are
passed directly, not through a shell. Calls disable service autostart, cap response size, drain
stdout/stderr and bound process lifetime. Read calls do not permit interactive authorization;
explicit writes allow the service's normal authentication policy. No permissive policy for
power profiles is installed by this app. Missing busctl/system bus/service disables this adapter,
while sysfs discovery remains available. A native Rust D-Bus transport can replace the adapter
without changing the typed service API.

Switching validates the advertised profile list and reads fresh state. It refuses to cancel
active application holds, and also refuses changes if hold information is unavailable. An
already-active profile is a no-op. Service-declared performance inhibition blocks performance
selection; degradation is reported. The result is read back after a write, with bounded waiting
and no repeated writes. Other clients can still change the profile afterwards.

`power test` temporarily selects an alternative advertised profile and attempts to restore the
original even after a failed test request. It does not override new holds. As with other live
controls, service failure, forced termination or a concurrent user action can interrupt or
prevent restoration. It is a deliberate test command, never run automatically at startup.

API references: [power-profiles-daemon](https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html)
and [busctl](https://www.freedesktop.org/software/systemd/man/latest/busctl.html).

## Preferences

The unprivileged app owns its preferences. The root helper never reads user config files,
config paths or environment overrides. It continues to accept only individual named requests.

Schema 1 stores an optional desired power profile and per-battery start/end charge thresholds.
Saving validates the schema, identifiers and ranges without requiring that this machine expose
those features. Portability is intentional: actual support is checked by `settings plan` and by
each explicit control request before application.

Storage follows the [XDG base-directory specification](https://specifications.freedesktop.org/basedir/latest/).
Relative XDG paths are ignored. Missing preferences return empty defaults without creating files.
The application directory is private and the file mode is 0600. Saves lock a protected local
file, write/sync a new temporary file, rename it atomically and sync the directory. Readers see
the previous or next complete file. A previous valid configuration is retained separately.
Malformed data and newer schema versions are preserved rather than reset. File/directory
symlinks, unsafe ownership and shared writable application directories are rejected for saves.

Saving is not activation. There is no boot/resume service, automatic preference replay, or
hidden application of saved settings. Those features require a separate authorized design
that coordinates with existing power managers and verifies hardware compatibility.

## Status and monitoring

`status` is a single JSON report joining hardware, inventory, power-service information, known
controllers and preferences. Invalid preferences are reported independently so hardware data
is still readable. `monitor <n>` emits 1–60 samples with a one-second pause between samples;
slow hardware/service reads can lengthen the actual sampling interval. It writes no log itself.

The report intentionally includes no serial numbers/UUIDs or account information. Power holds
can reveal application identifiers/reasons, so reports should be previewed before sharing.
