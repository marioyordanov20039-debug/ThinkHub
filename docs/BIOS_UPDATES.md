# ThinkHub BIOS update workflow

The Firmware tab can refresh the configured firmware catalogs, inspect BIOS upgrade readiness,
display a prominent warning, and prepare a reviewed BIOS release through fwupd. No BIOS has
been flashed as part of development. Live validation covers the T480 read-only plan and warning;
tests use a fake firmware transport for the installation path. Physical staging, reboot and
post-update recovery still need controlled hardware validation before a broad release.

Only one unambiguous internal UEFI-capsule device with a system-firmware (`X-System`) release
is accepted. EC, management engine, Secure Boot databases and peripherals are not selected as
the BIOS. Only stable `lvfs` releases marked trusted metadata and upgrade are offered, with a
SHA-256 checksum. Testing/alternate/blocked releases and arbitrary local files are not exposed.
The root-owned system fwupdmgr client must advertise exact device/version installation;
older clients without that command get an explanation rather than a guessed fallback.

The preflight requires confirmed external power, at least one present laptop battery, and
known charge of at least 30% on each present laptop battery. fwupd may enforce stricter vendor
requirements. Device problems, pending firmware updates and missing/stale/future-dated LVFS
metadata block preparation. The catalog must be no more than 24 hours old. Refresh is explicit,
downloads metadata from already-enabled remotes and does not install firmware or enable remotes.
No catalog refresh happens during ordinary status polling or a read-only BIOS check.

The warning explains that interrupting a BIOS update can prevent booting, requires AC power,
saved work and no interruption, and mentions TPM-backed disk-encryption recovery keys. A risk
acknowledgement is required to enable **Authenticate & prepare BIOS update**. Closing the
warning does not launch installation. The app rechecks the BIOS identity, current version,
release checksum/version, catalog age and power before launching the installation request.

Preparation always invokes `/usr/bin/pkexec --disable-internal-agent /usr/bin/fwupdmgr` with
validated device/version arguments. The normal GUI refuses to perform it when already running
as root. The system polkit authentication agent owns password entry: ThinkHub never collects,
receives, caches or stores passwords. Standard polkit `org.freedesktop.policykit.exec` uses
`auth_admin` without retained authorization. Administrators remain responsible for any custom
system authorization rules. Cancellation or denial returns an error before the privileged
firmware client is started. This does not add a permissive policy or change global policy.

The adapter validates upgrades because fwupdmgr's exact-version `install` command also supports
reinstalls/downgrades in general. Only the reviewed upgrade is passed; no force, no-safety-check,
allow-older, assume-yes or arbitrary-file options are supplied. The child starts from a trusted
working directory so fwupdmgr cannot mistake a user file named like a device ID for a local CAB.
fwupd retains its payload, compatibility, signing and hardware checks. ThinkHub's existing
sysfs helper does not gain firmware operations or additional privileges.

During authorization/preparation, the app shows an activity indicator and blocks closing the
main window. It requests logout/suspend inhibition where the desktop supports it. Output is
drained with bounded retained logs, and there is no kill timeout on firmware staging. There is
no retry, rollback or forced cancellation. `--no-reboot-check` prevents an automatic reboot;
completion explains the next restart and does not claim the BIOS has already been flashed.
Errors instruct the user to inspect status before retrying because staging can be partial.

Developer read-only command (new source build; no privileged CLI reinstall is needed by the GUI):

```sh
cargo run --bin thinkpad-dashboard -- firmware bios-plan
```

The serialized plan omits the fwupd device ID. Set `THINKHUB_CAPTURE_BIOS_REVIEW` to an absolute
PNG filename when launching a development GUI to open and capture only the warning. This
diagnostic never acknowledges the warning, refreshes metadata or launches installation.

References: [fwupd UEFI capsule behavior](https://fwupd.github.io/libfwupdplugin/uefi-capsule-README.html),
[fwupdmgr exact-version command implementation](https://chromium.googlesource.com/external/github.com/fwupd/fwupd/+/93ba530d07168a313f27529e4763675973df36d9/src/fu-util.c).
