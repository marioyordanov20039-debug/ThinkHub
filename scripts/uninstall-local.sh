#!/bin/sh
# Removes only the files installed by install-local.sh. No user settings or unrelated services.
set -eu
PATH=/usr/sbin:/usr/bin:/sbin:/bin
export PATH
if [ "$#" -ne 0 ] || [ "$(id -u)" -ne 0 ]; then
    echo 'Usage: run uninstall-local.sh with administrator authorization' >&2
    exit 1
fi
rm -f /usr/local/bin/thinkpad-dashboard /usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper /usr/share/polkit-1/actions/io.github.thinkpaddashboard.control.policy
rmdir /usr/lib/thinkpad-dashboard 2>/dev/null || true
echo 'Removed local ThinkPad Dashboard integration'
