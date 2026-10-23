# Product and implementation plan

## Goal

ThinkHub: a modern, understandable Rust desktop application for ThinkPads on major Linux distributions.
Keep layout design until the backend works. Prefer documented Linux and desktop service APIs
over model-specific hacks. Aim for broad ThinkPad coverage while accurately distinguishing
supported, detected but unverified, unavailable, failed, and planned features.

Perfect support for every ThinkPad cannot be promised: firmware, kernel drivers, optional
hardware, CPU architectures and desktop environments vary. A reliable experience includes
clear explanations when a feature is unavailable, not a button that pretends to work.

## Feature inventory

| Area | First backend milestone | Further work |
| --- | --- | --- |
| Batteries | Per-battery/aggregate telemetry, threshold/mode requests, presets and saved preferences | Carefully managed calibration, optional authorized startup persistence, hotplug and notifications |
| Cooling | Temperatures and fan RPM, validated firmware-auto request | Watchdog, thermal failsafe, fan curves and model validation |
| Performance | CPU discovery, firmware profile requests, power-service profile control and holds | More service adapters, CPU/EPP controls and deeper ownership coordination |
| Display and lighting | Display and keyboard brightness discovery/requests | Percentage-based brightness previews implemented; UI, ambient sensors and privacy-screen integration remain |
| Connectivity | rfkill read-only state | NetworkManager and BlueZ integration; hardware-block explanations |
| Hardware overview | Model, BIOS, CPU, RAM, graphics driver/IDs, drives and OS | Dock inventory and richer hardware labels; DRM outputs and Hyprland current output data are implemented |
| Firmware | BIOS view, fwupd catalog refresh, guarded BIOS plan and warned/password-authorized preparation | Controlled physical staging/reboot/recovery validation and older-client adapters |
| Input | Generic device classification and PS/2 TrackPoint sensitivity/speed controls | Touchpad/compositor settings, USB/I2C controls and persistent preferences |
| Diagnostics | JSON hardware snapshot, missing sensor notices | Friendlier diagnostics, report preview and more controller adapters |
| Desktop experience | Seven-page native ThinkHub prototype, light/dark themes, preset/custom accents and saved appearance | Final layout, accessibility, keyboard navigation, scaling and translations |

Not every ThinkPad feature is relevant to every model. Avoid implementing BIOS password
management or raw embedded-controller register writes as generic controls. Use established
services for firmware updates and desktop-owned settings.

## Milestones

1. **Backend foundation — implemented:** Rust library, CLI, capability discovery, validated
   request plans, development write helper, fixtures and live read-only T480 scan.
2. **Hardware control validation — in progress:** polkit action, trusted helper installation,
   serialized operations, known-systemd-service blockers and reversible keyboard test implemented.
   Saved user preferences and current/legacy power-service integration are now implemented.
   Remaining: optional authorized startup persistence, more adapters, broader live controls and
   full packaging review.
   Obtain hardware reports from older and newer Intel/AMD models and then ARM ThinkPads.
3. **Backend expansion:** missing hardware overview, service integrations, hotplug/resume,
   monitoring history (finite JSON sampling is implemented), fan safety and feature-specific error explanations.
4. **Interface design and implementation — in progress:** first native ThinkHub prototype
   implemented after backend validation. Refine the layout with actual use. The GUI reuses
   backend validation and calls the installed CLI; the ordinary app stays unprivileged.
5. **Packaging and release validation:** `.deb`, `.rpm`, Arch packaging; install/upgrade/remove
   tests; Ubuntu/Debian/Fedora/Arch runtime checks; X11/Wayland and desktop testing.
6. **Public release:** app name chosen as ThinkHub; choose a license, review source and docs, create GitHub repo,
   publish verified release artifacts/checksums and tested model/distro information.

## Release acceptance

- The app remains useful without root and without optional services.
- Unsupported features are explained and cannot be accidentally activated.
- Every exposed control validates its range, identifies its owner and reports actual errors.
- Fan control recovers safely after crashes/suspend before manual controls are shipped.
- Multi-battery machines, missing sensors, device removal and denied authorization are tested.
- Compatibility claims link to hardware evidence; simulated tests are labeled as simulations.
- Native packages work on supported distro versions and remove cleanly.
- Accessibility and responsive scaling are tested before release.
