# Install ThinkHub

This guide covers the first public preview, **v0.5.0-preview.1** (app/backend version 0.5.0). Native packages include the GUI, matching CLI, restricted polkit helper, TH icon and application-menu entry. **No AppImages.** All binaries are for x86_64/amd64 Linux. Do not install them on ARM, aarch64 or 32-bit machines.

## 1. Choose and download a native package

Download from [the GitHub release](https://github.com/marioyordanov20039-debug/ThinkHub/releases/tag/v0.5.0-preview.1) using your browser, or the commands below. Run these commands in an empty download directory. Do not use `sudo` for downloads.

### Ubuntu 22.04 or later, Linux Mint and other Ubuntu-based systems

Choose the Ubuntu package when the derivative is based on Ubuntu 22.04 or newer. Older bases need upgrading or a suitable source build. Elementary/Pop!_OS/Mint versions use different bases; check your distro's documentation rather than choosing by its name alone.

```sh
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/thinkhub_0.5.0-1_ubuntu22.04_amd64.deb
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS &&
sudo apt update &&
sudo apt install ./thinkhub_0.5.0-1_ubuntu22.04_amd64.deb
```

### Debian 12 or later and Debian-based systems

Use this package on Debian 12 (Bookworm), Debian 13 (Trixie) or a compatible derivative. For Ubuntu-based distributions use the Ubuntu build above. Package dependencies must resolve without forcing or ignoring them.

```sh
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/thinkhub_0.5.0-1_debian12_amd64.deb
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS &&
sudo apt update &&
sudo apt install ./thinkhub_0.5.0-1_debian12_amd64.deb
```

### Fedora 43 or later and compatible Fedora-based systems

This RPM is built on Fedora 43. It is not an openSUSE, RHEL or universal RPM package. On Fedora Atomic desktops, a layered install uses `rpm-ostree install ./<downloaded.rpm>` and normally needs a reboot; that path has not been validated for this preview.

```sh
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/thinkhub-0.5.0-1.fedora43.x86_64.rpm
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS &&
sudo dnf install ./thinkhub-0.5.0-1.fedora43.x86_64.rpm
```

### Arch Linux, CachyOS and EndeavourOS

Fully update your system before installing a rolling-release build. Manjaro and other delayed-repository derivatives may have older libraries: if dependencies are unavailable, wait for them or build from source. Never force library dependency checks.

```sh
sudo pacman -Syu
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/thinkhub-0.5.0-1-x86_64.pkg.tar.zst
curl -fLO https://github.com/marioyordanov20039-debug/ThinkHub/releases/download/v0.5.0-preview.1/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS &&
sudo pacman -U ./thinkhub-0.5.0-1-x86_64.pkg.tar.zst
```

The preview is not in the AUR or official distro repositories. Do not run `yay -S thinkhub` expecting this release. Packages are currently unsigned; SHA256SUMS detects a damaged/mismatched download, but is not an independent publisher signature. Only install assets from this repository's release page. **Stop if checksum verification fails.**

## 2. Launch and optional integrations

Search for **ThinkHub** in your desktop's application menu, or run:

```sh
thinkhub
```

Run it as your normal user, never `sudo thinkhub`. GTK4 works on GNOME, KDE and other desktops; a GNOME session is not required. A graphical Wayland or X11 session is required for the GUI. The CLI can operate without a GUI.

Reading ordinary hardware information does not require root. Applying supported privileged controls requires a working desktop **polkit authentication agent**. GNOME/KDE normally supply one. On minimal Hyprland/Sway sessions, configure your desktop's supported authentication agent. ThinkHub does not install a permissive policy, bypass authentication or start an agent for you.

Optional firmware support:

| Distro | Install fwupd |
| --- | --- |
| Ubuntu / Debian | `sudo apt install fwupd` |
| Fedora | `sudo dnf install fwupd` |
| Arch / CachyOS | `sudo pacman -S --needed fwupd` |

Installing fwupd is not a BIOS update. The BIOS feature additionally requires a recent client advertising exact-version `install [DEVICE-ID\|GUID] [VERSION]`, suitable LVFS firmware and a successful preflight. Older distro fwupd versions may support inventory but cannot use ThinkHub's guarded BIOS preparation. Do not replace vendor firmware tools or add third-party repositories just to bypass this restriction. See [the BIOS workflow](BIOS_UPDATES.md).

Power-profile switching uses an already-running service compatible with power-profiles-daemon and the optional `busctl` command from systemd. Fedora's tuned provider can supply that API. ThinkHub does not replace/start/stop your power manager. Do not install multiple competing power managers to enable a button. Sensor and battery interfaces depend on kernel drivers such as `thinkpad_acpi`; ThinkHub does not change module parameters automatically.

## Updates and uninstall

The app has no self-updater. Download a newer matching package and install it using the same package-manager command. The GUI, backend and helper should be upgraded together. This preview's binaries report 0.5.0; the preview tag identifies the release.

Remove the native package with:

```sh
# Ubuntu / Debian
sudo apt remove thinkhub
# Fedora
sudo dnf remove thinkhub
# Arch / CachyOS
sudo pacman -R thinkhub
```

Only use the command for your distro. Package removal does not delete user preferences in `~/.config/thinkhub` and `~/.config/thinkpad-dashboard`. It does not remove unrelated power/firmware services or undo hardware settings already applied.

### If you used the earlier developer installer

The developer install is separate from the package-managed install and lives under `/usr/local` and `~/.local`. Before switching, close ThinkHub, use the old checkout's `sudo sh scripts/uninstall-local.sh`, and remove only its user launcher/binary:

```sh
rm -f "$HOME/.local/bin/thinkhub" "$HOME/.local/share/applications/io.github.thinkhub.ThinkHub.desktop"
```

These two paths are the files created by the developer installer; the command does not remove your settings. Remaining user TH icon files are harmless. Native packages use `/usr/bin/thinkhub` and `/usr/bin/thinkpad-dashboard`; the GUI is built to use that backend. A user desktop entry can otherwise shadow the system launcher.

## Build from source

For unsupported distro versions or development, build natively on your distro. Required: Rust **1.85 or newer**, GTK4 development libraries, a C linker, Python 3 and Git. Network access is needed to fetch Rust crates during the first build.

### Build dependencies

```sh
# Ubuntu / Debian
sudo apt update
sudo apt install build-essential git curl ca-certificates pkg-config libgtk-4-dev python3

# Fedora
sudo dnf install gcc git curl ca-certificates pkgconf-pkg-config gtk4-devel python3 libglvnd-gles mesa-dri-drivers

# Arch / CachyOS
sudo pacman -Syu --needed base-devel git curl ca-certificates pkgconf gtk4 python
```

Run only your distro's block. Install Rust through your distro if it provides Rust 1.85+, or follow [the official Rust installer](https://www.rust-lang.org/tools/install). Build as your normal user, never with sudo.

```sh
git clone --branch v0.5.0-preview.1 --depth 1 https://github.com/marioyordanov20039-debug/ThinkHub.git
cd ThinkHub
cargo test --locked
cargo test --manifest-path gui/Cargo.toml --locked
cargo build --release --locked
cargo build --manifest-path gui/Cargo.toml --release --locked
sudo sh scripts/install-local.sh
python3 scripts/install-gui.py
```

Launch `~/.local/bin/thinkhub`. This developer/source install puts the root-owned backend in `/usr/local/bin` and the GUI in `~/.local/bin`, with a per-user launcher. Do not set `THINKHUB_CLI_PATH` for this workflow. No ThinkHub service is installed.

To build native packages instead, set `THINKHUB_CLI_PATH=/usr/bin/thinkpad-dashboard` **while building the GUI** and use `scripts/package-native.py` on the target distro. `packaging` prerequisites and exact commands are maintained in [.github/workflows/packages.yml](../.github/workflows/packages.yml). Do not relabel a binary built on Arch as an Ubuntu/Fedora build. Dependencies and minimum glibc versions are distro-specific.

Developer install removal: `sudo sh scripts/uninstall-local.sh`, followed by the two-file removal in the migration section above. Keep your checkout if you need the uninstall script.

## Troubleshooting

- **The GUI cannot find the backend / version mismatch:** reinstall the complete matching package. Source installs need both Rust builds and both installers. Never run a downloaded helper by itself with elevated privileges.
- **Duplicate/old launcher:** see the developer-install migration section, then log out/in if your launcher caches entries.
- **No authorization dialog:** check your desktop's polkit agent. Do not run the whole GUI as root.
- **A control is blocked:** read its conflict message. TLP, tuned, Vantage, thinkfan or another service may own that interface. ThinkHub will not stop them automatically.
- **Missing battery/fan/TrackPoint controls:** detection is capability-based. Some ThinkPads expose different interfaces or no control at all. Missing does not mean the package installed incorrectly.
- **BIOS preparation unavailable:** check AC, each battery, catalog age and the fwupd version. Read-only inventory does not prove update support. No physical firmware flash was validated for this preview.
- **Non-systemd Linux:** basic sysfs monitoring may work; service detection and optional transports can be unavailable and affected writes stay blocked. This preview does not claim full non-systemd support.

When reporting a bug, include distro/version, kernel, desktop and ThinkPad model without serial number. Review command output before posting it. [Compatibility and known limits](COMPATIBILITY.md) distinguish actual hardware tests from container checks.
