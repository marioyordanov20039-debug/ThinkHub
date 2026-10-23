#!/usr/bin/env python3
"""Smoke-test local installation in a disposable tree; never requests authorization."""
import pathlib
import stat
import subprocess
import tempfile
import xml.etree.ElementTree as ET

project = pathlib.Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="thinkpad-dashboard-staging-") as temporary:
    root = pathlib.Path(temporary)
    subprocess.run(["sh", str(project / "scripts/install-local.sh"), "--destdir", str(root)], check=True)
    for binary, relative in [
        ("thinkpad-dashboard", "usr/local/bin/thinkpad-dashboard"),
        ("thinkpad-dashboard-helper", "usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper"),
    ]:
        installed = root / relative
        assert installed.is_file() and not installed.is_symlink()
        assert stat.S_IMODE(installed.stat().st_mode) == 0o755
        assert installed.read_bytes() == (project / "target/release" / binary).read_bytes()
    policy = ET.parse(root / "usr/share/polkit-1/actions/io.github.thinkpaddashboard.control.policy")
    assert policy.find(".//allow_active").text == "auth_admin"
    assert policy.find(".//allow_any").text == "no"
    assert policy.find(".//allow_inactive").text == "no"
    assert policy.find(".//annotate").text == "/usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper"
    assert not list(root.rglob("*.service"))
print("Staged CLI, helper and restrictive polkit policy verified; no daemon installed.")
