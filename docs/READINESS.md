# ThinkHub backend and first interface prototype

The first native ThinkHub interface now lives in `gui/`. See [GUI.md](GUI.md) for build,
installation, behavior and prototype limits. The internal backend package name remains
`thinkpad-dashboard`. GUI work does not replace the remaining release checks below.

The core backend can support a first graphical prototype. This is not a claim that every
ThinkPad control, desktop environment, distribution or firmware combination is implemented.
The interface must distinguish tested controls, detected interfaces, conflicts and planned work.

| Area | Available to a first interface | Remaining work before a broad release |
| --- | --- | --- |
| Hardware monitoring | Model/BIOS, CPU/RAM/drives/graphics, batteries, sensors and fan RPM | More physical models, sensor validation/labels and richer hardware names |
| Power profiles | Current/legacy service APIs, holds, validation and readback | More providers, suspend/restart behavior and notifications |
| Battery controls | Threshold/mode requests, presets, previews and conflict blockers | Live writes after ownership is explicitly resolved, more models and controlled calibration |
| Lighting | Raw brightness controls and display percentage previews | More live display tests and polished percentage UI |
| Input | Generic discovery, PS/2 TrackPoint parameters and reversible test | GNOME/KDE/Hyprland desktop preferences, USB/I2C adapters and persistence |
| Displays | Generic connectors/modes; Hyprland current output metrics | Other compositor adapters and intentional display configuration workflows |
| Firmware | BIOS view, fwupd inventory, explicit catalog refresh and warned/authenticated BIOS preparation | Physical staging and post-reboot validation, recovery, more models and older fwupd adapters |
| Cooling | Fan monitoring and firmware-auto request | Watchdog, thermal failsafe, crash/suspend recovery and validated manual fan controls |
| Preferences | Private atomic saves, backup and explicit previews | Optional authorized startup/resume application and ownership coordination |
| Authorization | Root-owned helper, polkit, operation serialization, version matching | Package-manager upgrade/remove checks, security review and wider policy testing |

Manual fan curves, BIOS edits and automatic startup application must not be
presented as working actions. Missing firmware/desktop interfaces must not be replaced by
guessed model-specific writes. Other applications are not stopped automatically to clear a conflict.

BIOS preparation delegates to fwupd; it has read-only live validation and fake-transport tests,
not a completed physical flash. See [BIOS_UPDATES.md](BIOS_UPDATES.md).

The first public preview includes native packages and an MIT license. Remaining work includes
broader distro runtime/security-policy checks, ARM and older model coverage, accessibility/scaling,
physical firmware validation and additional hardware controls. See [RELEASE.md](RELEASE.md).
