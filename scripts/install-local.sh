#!/bin/sh
# Development integration only; native distro packages will own production installation.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
destination=
verify_keyboard=false
verify_bios=false
verify_trackpoint=false
if [ "${1-}" = "--destdir" ] && [ "$#" -eq 2 ]; then
    destination=$2
    case "$destination" in /*) ;; *) echo 'DESTDIR must be absolute' >&2; exit 1;; esac
elif [ "$#" -eq 1 ] && [ "$1" = "--verify-keyboard" ]; then
    verify_keyboard=true
elif [ "$#" -eq 1 ] && [ "$1" = "--verify-bios" ]; then
    verify_bios=true
elif [ "$#" -eq 1 ] && [ "$1" = "--verify-trackpoint" ]; then
    verify_trackpoint=true
elif [ "$#" -ne 0 ]; then
    echo 'Usage: install-local.sh [--destdir /absolute/staging/directory | --verify-keyboard | --verify-bios | --verify-trackpoint]' >&2
    exit 1
fi
if [ -z "$destination" ] && [ "$(id -u)" -ne 0 ]; then
    echo 'Administrator authorization is required for system installation' >&2
    exit 1
fi
for binary in thinkpad-dashboard thinkpad-dashboard-helper; do
    test -x "$project/target/release/$binary" || { echo 'Run cargo build --release --locked first' >&2; exit 1; }
done
install -d -m 0755 "$destination/usr/lib/thinkpad-dashboard" "$destination/usr/local/bin" "$destination/usr/share/polkit-1/actions"
# install makes fresh root-owned copies in the real installation, never symlinks to a user build.
install -m 0755 "$project/target/release/thinkpad-dashboard" "$destination/usr/local/bin/thinkpad-dashboard"
install -m 0755 "$project/target/release/thinkpad-dashboard-helper" "$destination/usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper"
install -m 0644 "$project/data/io.github.thinkpaddashboard.control.policy" "$destination/usr/share/polkit-1/actions/io.github.thinkpaddashboard.control.policy"
echo "Installed ThinkPad Dashboard CLI, helper and polkit action under ${destination:-/}"
if [ "$verify_keyboard" = true ]; then
    /usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper --test-keyboard
fi
if [ "$verify_bios" = true ]; then
    /usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper --bios-info
fi
if [ "$verify_trackpoint" = true ]; then
    /usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper --test-trackpoint
fi
