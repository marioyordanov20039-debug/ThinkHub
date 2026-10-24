# ThinkHub native prototype

ThinkHub is the app name. The existing internal backend and privileged helper keep their
`thinkpad-dashboard` names for now. The GUI is a separate Rust crate in `gui/`, using GTK4
with the GTK 4.0 baseline and Rust 1.85. It does not require a GNOME desktop session.

Build and install for your normal user:

```sh
cargo build --manifest-path gui/Cargo.toml --release --locked
python3 scripts/install-gui.py
```

Launch **ThinkHub** from the app menu, or run `~/.local/bin/thinkhub`.
The default app icon is the separated white TH monogram, graphite tile and red dot above the H.
The installer includes its scalable SVG and 16–512px PNG fallbacks and refreshes the desktop/icon
caches where the tools are available. Future distro packages must include the same app-ID assets.
Install the matching 0.5.1 backend using `scripts/install-local.sh` first. Developer builds call the
fixed `/usr/local/bin/thinkpad-dashboard` path; native packages use a build-time `/usr/bin/thinkpad-dashboard` path and checks its status schema and version.
It never runs itself as root. Hardware writes still use the installed root-owned helper
and the system's polkit prompt. No AppImage or background daemon is installed.

Development dependencies: a Rust compiler, C linker, pkg-config and GTK4 development files.
Typical package names are `build-essential pkg-config libgtk-4-dev` on Debian/Ubuntu,
`gcc pkgconf-pkg-config gtk4-devel libglvnd-gles mesa-dri-drivers` on Fedora and `base-devel pkgconf gtk4` on Arch.
These are instructions for future testing, not evidence of validation on those systems.
Native preview packages and source installation are documented in [INSTALL.md](INSTALL.md).

The initial layout has Overview, Batteries, Power & thermals, Display & input, Firmware,
Preferences & diagnostics, Customize, and Feedback pages. Live snapshots are read on a worker thread every five
seconds. GTK stays on the main thread. Editors retain drafts between refreshes, unless
hardware identity or control ownership changes. Failed reads mark displayed data as stale
and prevent actions until a successful refresh. Device changes recreate affected editors.

Hardware controls first request a fresh preview. Only an unblocked preview offers Apply;
the backend checks again when applying. Competing battery controllers disable battery
buttons and explain the conflict. Power-profile changes ask for confirmation and use the
power service's policy and hold checks. Firmware scans are explicit, read-only cached
information; empty results do not mean firmware is current. Preferences have ordinary
form fields; saving never applies hardware settings or adds startup automation.

Customize offers dark/light themes, eight accent presets, a custom color picker and reset to
defaults. Appearance changes apply live without rebuilding hardware editors and are saved
atomically to `$XDG_CONFIG_HOME/thinkhub/appearance.json` (normally
`~/.config/thinkhub/appearance.json`). This is separate from backend power preferences and
does not alter the desktop theme. Accent text and button foregrounds adapt for contrast.
Malformed or unsupported settings fall back to defaults with an explanation in Customize;
save failures leave a live preview and an explicit error. Customize works even without the backend.

Read-only visual smoke check (navigate all eight native pages and export their rendered window):

```sh
THINKPAD_DASHBOARD_CAPTURE_DIR=/absolute/output/directory gui/target/release/thinkhub
```

Run this without an existing ThinkHub instance. It renders only its own window and never
clicks hardware controls. The window returns to Overview after capture. Screenshots may
contain model and device names, so keep diagnostic captures local unless intentionally shared.

For a native appearance integration check, additionally set `THINKHUB_TEST_CUSTOMIZE=1`
and point `XDG_CONFIG_HOME` at an isolated test directory. This opt-in mode triggers the
Light button and Blue preset through their GTK signals, writes appearance in that test
directory, and captures the live result as `customize-light-blue.png`. It never triggers
hardware controls. Relaunch with the same test directory to verify appearance restoration.

Validation commands:

```sh
cargo fmt --manifest-path gui/Cargo.toml --check
cargo test --manifest-path gui/Cargo.toml --locked
cargo clippy --manifest-path gui/Cargo.toml --all-targets --locked -- -D warnings
```

This is the first working UI, not the final layout. Accessibility, translations, adaptive
narrow layouts, additional model testing and distro packaging remain release work. The
backend's previously documented limits still apply: no manual fan curves, BIOS edits,
raw firmware flashing or automatic startup application. Supported BIOS preparation now uses
the [fwupd workflow](BIOS_UPDATES.md), with warning, acknowledgement and system authorization;
physical flashing/reboot has not been tested. This preview is released with those limits documented.

The [Feedback tab](FEEDBACK.md) opens reviewed email drafts addressed to info.thinkhub@protonmail.com.
It also works without the hardware backend. Sending remains an explicit action in the user’s email app.
