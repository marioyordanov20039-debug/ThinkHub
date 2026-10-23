# Battery modes and firmware information

## Charging controls

The kernel `charge_behaviour` attribute advertises supported values with the selected value in
brackets. The parser requires exactly one selection, valid identifiers and no duplicates.
Controls support `auto` and `inhibit-charge` only when advertised. Kernel lists may contain
other modes, but a list entry alone does not enable a corresponding write request in this app.
Forced discharge/calibration need a separately managed workflow and are not implemented.

The request planner normalizes the selected mode for before/after values. Concurrency checks
and readback use the same normalization rather than comparing the full list to a write token.
The helper validates battery type/presence and conflicts again before writing. Unit tests
simulate kernel-style list updates without altering a physical battery. Live previews on the
T480 are deliberately not applied while an existing Vantage service owns battery controls.

Presets are example threshold pairs, not model-specific recommendations or guaranteed firmware
limits. Selecting a preset creates a preview request. It never saves or activates that request
automatically. A normal explicit apply action still needs authorization and ownership checks.

Combined laptop charge is total remaining energy divided by total full energy, when all
included batteries expose those values. It is not the mean of per-battery percentages.
Peripheral `scope=Device` batteries are omitted from laptop totals. Discharge estimates use
remaining energy divided by positive total battery power only with complete data and known
discharging/idle states. These are instantaneous estimates, not a promise of remaining runtime.
Reported state of health and capacity-ratio health remain distinct measurements. A driver
cycle count of zero is unknown under the documented power-supply ABI. Programmable power
supplies with `online=2` count as online.

Reference: [Linux power-supply ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-class-power).

## BIOS information

Only a fixed set of descriptive, non-password settings is read from the optional `thinklmi`
firmware-attributes interface. Examples include Secure Boot, virtualization, input options,
Always On USB and wake settings. This does not enumerate authentication/password attributes.
Reads which need elevated permission remain unknown in the ordinary view. The explicit
authorized view runs the same read-only allowlist in the existing root helper. There is no
generic setting name/path parameter and no BIOS write request.

Kernel exposure varies by model and firmware. A setting shown here is firmware configuration,
not proof that an associated operating-system feature is active. A pending-reboot attribute,
when readable, is reported separately. BIOS editing is always marked unsupported by this app.

## Firmware service information

The optional fwupd adapter runs only the fixed `get-devices` and `get-updates` queries in JSON
mode. Metadata checks, remote setup prompts, history-report prompts and authentication prompts
are disabled for these queries. The explicit scan may D-Bus-activate an existing fwupd service.
It does not run refresh, download, install, update, security-fix, reboot or set-bios-setting.
Normal status/monitoring never invoke the fwupd client.

Responses are bounded by time and size. Only names, vendors, versions, updatable status and
available version lists are returned. Unique ID fields, instance IDs, serials, descriptions
and raw command output are not exported. Unrecognized optional flag formats remain unknown.
Exit 2 is accepted only alongside valid structured output; missing/malformed update data is
not turned into an empty update list.

Cached metadata has unknown freshness. No available cached releases does not mean the machine
is fully updated. Public firmware updates need an explicit, source-backed confirmation flow,
appropriate power/reboot handling, supported fwupd APIs and model testing before implementation.
The separate [BIOS update workflow](BIOS_UPDATES.md) now implements explicit catalog refresh,
preflight, warning and authenticated preparation through fwupd. It does not change the read-only
behavior described above. Physical flashing and reboot validation have not been performed.

Reference: [fwupdmgr client documentation](https://fwupd.github.io/libfwupdplugin/fwupdmgr.html).
