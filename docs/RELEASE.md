# ThinkHub 0.5.1 feedback preview

Release tag: `v0.5.1-preview.1`. App, backend and helper version: `0.5.1`.
Native package version: `0.5.1-1`. These are preview builds, not a guarantee of universal ThinkPad support.

## Release gates

- 75 core and nine GUI tests, formatting and warning-free Clippy on Rust 1.85.
- Native builds in Ubuntu 22.04, Debian 12, Fedora 43 and rolling Arch containers.
- Each package installs with its distro package manager, its desktop entry validates and its
  helper is root-owned, mode 0755, without setuid/setgid permissions.
- Installed CLI read-only scan and GUI launch under a virtual X display; no hardware writes,
  BIOS refresh, authentication or firmware staging during CI.
- Public source excludes local reports, raw hardware dumps, credentials, build trees and user settings.
- README includes actual screenshots and links to install instructions, feature limits and BIOS warnings.
- Release assets come from the passing package workflow for the release commit; SHA256SUMS lists them.
- GitHub pre-release flag remains enabled. Real hardware testing currently covers T480/CachyOS only.

See the repository's Actions runs for execution results. A configured workflow is not a passed
check, and CI's empty virtual hardware is not a physical compatibility result.

## Reproduce native package builds

Use the distro prerequisites and Rust setup in [packages.yml](../.github/workflows/packages.yml).
Build the backend with `cargo build --release --locked`, then the GUI with:

```sh
THINKHUB_CLI_PATH=/usr/bin/thinkpad-dashboard cargo build --manifest-path gui/Cargo.toml --release --locked
```

Run `python3 scripts/package-native.py --format deb --baseline ubuntu22.04` on Ubuntu 22.04;
use `--format deb --baseline debian12`, `--format rpm --baseline fedora43`, or
`--format arch --baseline arch` on those actual distributions. Outputs go to ignored `dist/`.
Never build a release GUI with the developer `/usr/local/bin` backend path.

Native packages use `/usr/bin/thinkhub`, `/usr/bin/thinkpad-dashboard`,
`/usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper` and the restricted polkit action.
They install icons and the launcher under `/usr/share`. No ThinkHub service is installed.

Packages are unsigned in this first preview. A release checksum manifest detects mismatched
files but is not an independently authenticated signature. Reproducible byte-for-byte builds,
artifact attestations, ARM builds and broader physical validation remain future work.

Version 0.5.1 adds Feedback with reviewed mailto drafts, optional allowlisted system details,
validation and a copy fallback. Receipt of the generated test message was confirmed before
publication. Test-only mailto handlers capture drafts in CI; CI never sends real feedback mail.
