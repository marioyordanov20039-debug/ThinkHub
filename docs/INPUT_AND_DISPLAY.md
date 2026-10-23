# Input devices and displays

## Discovery

Input enumeration uses `/sys/class/input/event*`; the corresponding mouse/input aliases are
not enumerated again. Classification prefers selected udev `ID_INPUT_*` flags. Kernel input
properties/capabilities are fallbacks, and name-based hints are explicitly labeled as hints.
Raw udev records, physical paths, unique identifiers and capability bitmaps are not returned.
The app never opens `/dev/input/event*`, captures keystrokes/movement, or grabs a device.
Event/controller identifiers are snapshot-local and are not stored as persistent preferences.

DRM connector status, enabled state and mode-resolution strings are read from sysfs. Missing
or unknown fields stay unknown. Duplicate dimensions can represent different refresh rates,
so they are deduplicated without inferring a refresh rate or preferred/current/native mode.
EDID is not read, avoiding its serial/unique identifier fields.

The optional read-only Hyprland adapter is selected only when the desktop environment reports
Hyprland. It uses fixed `hyprctl` queries with bounded output/time and returns selected current
output metrics and pointer names. It does not expose monitor serials, device addresses, raw
descriptions or session socket identifiers. Other desktops keep kernel discovery; no installed
GNOME/KDE configuration tool is invoked merely because it is present on the system.

## Controls

TrackPoint requests expose only sensitivity and speed, 1–255. They require a PS/2 controller,
the psmouse driver, the kernel pointing-stick property, and an existing valid numeric attribute.
Identity does not depend on a particular vendor name. The driver/property check is repeated
before applying the planned write; generated sysfs paths cannot escape the hardware root.
USB/I2C pointing sticks can still appear in discovery but have no PS/2 control path.

These are hardware parameters, distinct from compositor/libinput acceleration. A request does
not edit desktop configuration or persistence. Driver readback verifies its parameter, not
physical cursor response; some driver attributes are cached, and hotplug or other software
can reset them. Remaining race/error behavior is the same as other sysfs controls.

The reversible sensitivity test requires exactly one verified PS/2 pointing stick. It changes
one step, verifies the parameter and attempts restoration even after a reported test failure.
Forced termination or hardware removal can interrupt restoration. The test performs no input
event recording or synthetic input. It is never run automatically during startup.

Percentage brightness produces a normal brightness request by rounding to the discovered range.
The percentage command only previews; applying the request is a separate authorized action.
Resolution, refresh rate, scale, display arrangement, touchpad preferences and desktop pointer
acceleration are not changed by this milestone.

## Helper compatibility

The ordinary app checks that its installed root-owned helper reports the same version before
requesting polkit authorization. A read-only `--version` query requires no administrator rights.
Older helpers without that operation, mismatched versions and malformed responses fail closed.
The development installer upgrades both binaries together. This is a conservative prototype
compatibility rule; a future public protocol may support explicit compatible version ranges.

References: [TrackPoint sysfs ABI](https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-devices-platform-trackpoint),
[kernel TrackPoint driver](https://github.com/torvalds/linux/blob/master/drivers/input/mouse/trackpoint.c),
[DRM connector properties](https://docs.kernel.org/gpu/drm-kms.html),
[Hyprland device configuration](https://wiki.hypr.land/configuring/core/devices/).
