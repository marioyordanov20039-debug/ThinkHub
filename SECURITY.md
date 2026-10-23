# Security

ThinkHub is an early preview with privileged hardware integration. The 0.5 preview is the only
currently maintained line. Review the [authorization architecture](docs/ARCHITECTURE.md) and
[BIOS workflow](docs/BIOS_UPDATES.md) before making changes to privileged operations.

Please use GitHub's **Report a vulnerability** under this repository's Security tab for a private
security report. Do not put exploit details, passwords, serial numbers or recovery keys in public
issues. If private reporting is temporarily unavailable, open a minimal issue requesting a private
contact without disclosing the vulnerability. Reports should describe affected versions, impact,
reproduction steps using simulated hardware where possible, and any suggested mitigation.

The GUI must remain unprivileged. Hardware writes go through validated, version-matched system
components and polkit. The app never collects system passwords. Package checksums verify file
integrity; this preview does not provide an independent signing identity or claim a security audit.
