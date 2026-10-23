# Compatibility record

Last local verification: 2026-10-24. Do not treat configured CI or simulated hardware as a
tested physical model or installed distro.

## Physical hardware

| Item | Result |
| --- | --- |
| Model | Lenovo ThinkPad T480, machine type 20L6SCGX00 |
| OS | CachyOS, Arch family, x86_64 |
| Kernel | 7.2.11-1-cachyos |
| Rust | Tests/Clippy/format checks passed on 1.85.0 and 1.99.0; release build on 1.99.0 |
| Read-only snapshot | Passed, no root |
| BAT0 / BAT1 | Both discovered; energy-based health approximately 47.5% / 80.8% at scan time |
| Temperatures / fan RPM | Discovered dynamically; unreadable sensor reported as a diagnostic |
| Battery threshold interfaces | Both detected; change previews validated, no real writes tested |
| Display / keyboard brightness | Interfaces detected; keyboard change preview validated |
| Firmware platform profile | Not exposed on this T480; correctly reported unavailable |
| Power-service profiles | Current API discovered; Performance → Balanced → Performance live test passed, restoration verified |
| Charging modes/presets | Both batteries advertise auto/pause controls; four live previews validated, no battery writes performed |
| Aggregate battery data | Two laptop batteries detected; capacity-weighted energy totals available |
| Firmware inventory | 22 components enumerated read-only; six components have cached update entries, metadata freshness unknown |
| BIOS view | Interface detected; normal-user restrictions respected; authorized allowlisted read passed, no BIOS editing |
| BIOS update workflow | T480 preflight found cached 0.1.44 → 0.1.56; stale catalog correctly blocks preparation. Native warning captured, fake-transport guards tested. No catalog refresh, authentication or physical staging/flash run during development. |
| Saved preferences | Save/load/backup/preview passed in temporary XDG directory; actual user preferences not created |
| Portable x86_64 backend | Rust 1.85 musl build passed; no ELF interpreter/shared libraries; live read-only T480 scan passed |
| Display/input discovery | Connected eDP panel, touchpad, TrackPoint and keyboard identified; Hyprland reports 1920×1080, 60.031 Hz, scale 1.05 |
| PS/2 controls | Sensitivity 200 and speed 97 discovered; request preview passed; live sensitivity test 200 → 201 → 200 passed; speed unchanged at 97 |
| Helper compatibility | Older helper rejected before authorization; installed helper 0.5 matches app 0.5; query requires no authorization |
| Status monitoring | Two complete JSON samples captured successfully |
| ThinkHub native GUI | Rust 1.85 GTK4 build, six GUI tests and Clippy passed; seven pages including Customize; local Hyprland/T480 validation |
| Firmware fan auto | Interface detected; no-op preview succeeded because firmware auto was already set |
| CPU policies / radio state | Read-only discovery succeeded |
| CPU / RAM / graphics / drives / OS | Expanded inventory read successfully |
| Known competing services | Vantage and power-profiles-daemon detected; overlapping battery request blocked before authorization |
| Installed integration | Root-owned CLI/helper and registered restrictive polkit action verified; installed v0.5 binaries match tested build; protected BIOS read passed in v0.4 |
| Keyboard self-test | Passed after asynchronous readback fix: brightness 2 → 1 → 2; restoration verified |
| Development helper privilege boundary | Non-root apply request rejected |
| Hardware settings changed during verification | Battery/cooling settings unchanged; keyboard test returned to original value |

Raw local reports are generated under `reports/` and excluded from Git. They are never uploaded.

## Simulated hardware tests

75 core tests and six GUI tests passed across library and integration tests. Cases include dual energy-based batteries,
older charge-based batteries, zero design capacity, absent batteries with stale attributes,
malformed readings, variable hwmon indices, invalid temperature sentinels, modern platform
profiles, AMD CPU policy information without a firmware profile, threshold update ordering,
bounds, no-op planning, readback mismatch, concurrent changes, partial failure, brightness
updates, rejected unknown request fields, path traversal, symlink escape, privacy and CLI errors.
Additional tests cover known controller conflicts, unknown service status, helper trust, operation
locking, insecure lock paths, CPU/memory/storage inventory and delayed LED readback.
Version 0.3 adds synthetic current/legacy service cases, profile holds, refused/invalid profile
changes, restoration after errors, atomic preferences, backups, schema validation, relative XDG
paths, unsafe settings paths, CLI save-without-activation and invalid monitoring arguments.
Version 0.4 adds charging-list parsing/write readback, rejected unsupported modes, energy-weighted
aggregate charge, peripheral battery exclusion, missing runtime data, reported health separation,
zero-cycle handling, programmable AC state, firmware sanitization, BIOS field allowlisting, cached
update parsing and subprocess size/time bounds.
Version 0.5 adds DRM mode deduplication, input alias/classification checks, PS/2 driver/property
verification, brightness conversion, unplug handling, compositor output validation/sanitization,
unknown-desktop handling, reversible TrackPoint restoration, request allowlisting and helper
version discovery/validation. No input events are read by these tests or the application.

## Distro and model matrix

| Target | Current evidence | Required before claiming release support |
| --- | --- | --- |
| Arch / CachyOS | Local build, tests and live read-only T480 run passed | Native package CI plus control validation on more physical models |
| Ubuntu 22.04 / 24.04 | Native build/install/reinstall/remove and seven-page GUI smoke CI; see release workflow | Additional physical runtime/security-policy checks |
| Debian 12 | Native build/install/reinstall/remove and seven-page GUI smoke CI; see release workflow | Additional physical runtime/security-policy checks |
| Fedora | Native build/install/reinstall/remove and seven-page GUI smoke CI; see release workflow | Additional physical runtime/security-policy checks |
| Other Intel / AMD ThinkPads | Synthetic interface cases only | Physical hardware and firmware-specific feature reports |
| ARM ThinkPads | No testing yet | Build/driver availability and physical hardware testing |
| Legacy IBM ThinkPads | Generic charge-based fixture coverage only | Model identity, older driver interfaces and physical testing |

Container CI validates compilation, synthetic tests, native package install/reinstall/removal and
seven-page GUI rendering. It does not validate host hardware, real desktop polkit prompts,
suspend/resume or physical BIOS updates. The public preview provides distro-specific native packages; see [INSTALL.md](INSTALL.md).

## Published preview verification

`v0.5.0-preview.1` was built from `13c897849880d433f11e710919b669229fc86c95`.
The [native package workflow](https://github.com/marioyordanov20039-debug/ThinkHub/actions/runs/40041149222)
passed all four distro jobs, including install, seven-page GUI rendering, reinstall and removal.
The [Rust/GUI workflow](https://github.com/marioyordanov20039-debug/ThinkHub/actions/runs/40041149212)
also passed. These container results do not expand the physical model validation above.
