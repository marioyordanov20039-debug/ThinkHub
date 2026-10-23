# Contributing to ThinkHub

Thank you for helping make ThinkHub useful across ThinkPad generations and Linux distributions.
Start with [installation](docs/INSTALL.md), [architecture](docs/ARCHITECTURE.md) and [compatibility](docs/COMPATIBILITY.md).

Open an issue for a bug, a missing interface or a feature proposal. Include the ThinkPad model,
distro/version, kernel, desktop, app version, reproduction steps and expected/actual behavior.
Remove serial numbers, UUIDs, usernames, credentials and firmware identifiers from attachments.
Report physical verification separately from simulated tests; a detected attribute is not a proven write.

Changes should discover capabilities, preserve unavailable states, preview mutations, respect
controller conflicts and keep authorization in the system-owned helper/service. Never add arbitrary
sysfs paths, shell commands, permissive polkit rules or guessed model-specific writes. Hardware
writes and firmware flashing are never required to run the tests.

Run the tests, Clippy and formatting commands in README.md. Add meaningful fixture tests for
behavior changes, especially validation, failure paths and restoration. Keep UI work on the GTK
main thread and blocking work off it. Package changes should pass all distro builds.

Submit a pull request explaining the problem, resulting behavior and validation. Keep unrelated
changes separate. Contributions are licensed under the project's MIT License.
