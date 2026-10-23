#!/usr/bin/env python3
"""Install ThinkHub for the current user; never installs or modifies the root helper."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
APP_ID = "io.github.thinkhub.ThinkHub"


def install_file(source, destination, mode):
    destination.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".thinkhub-", dir=destination.parent)
    os.close(fd)
    try:
        shutil.copyfile(source, temporary)
        os.chmod(temporary, mode)
        os.replace(temporary, destination)
    finally:
        Path(temporary).unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, help="Alternate user prefix (also useful for staging)")
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error("Run this as your normal desktop user, without sudo.")
    prefix = args.prefix or Path.home() / ".local"
    data = prefix / "share"
    if args.prefix is None and os.environ.get("XDG_DATA_HOME"):
        data = Path(os.environ["XDG_DATA_HOME"])
    for directory in (prefix, data):
        if not directory.is_absolute() or any(c in str(directory) for c in "\n\r\x00"):
            parser.error("Installation directories must be absolute paths without control characters")
    binary = ROOT / "gui/target/release/thinkhub"
    if not binary.is_file():
        parser.error("Build first: cargo build --manifest-path gui/Cargo.toml --release --locked")
    icons = [(ROOT / f"gui/assets/{APP_ID}.svg", Path(f"icons/hicolor/scalable/apps/{APP_ID}.svg"))]
    for size in (16, 24, 32, 48, 64, 128, 256, 512):
        icons.append((ROOT / f"gui/assets/icons/{size}x{size}/{APP_ID}.png",
                      Path(f"icons/hicolor/{size}x{size}/apps/{APP_ID}.png")))
    for source, _ in icons:
        if not source.is_file():
            parser.error(f"App icon asset is missing: {source}")
    target = prefix / "bin/thinkhub"
    install_file(binary, target, 0o755)
    for source, relative_destination in icons:
        install_file(source, data / relative_destination, 0o644)
    # Desktop Exec uses its own quoting rules, not a shell. Escape both parsing layers.
    quoted = str(target).replace("%", "%%")
    for character in ('\\', '"', '`', '$'):
        quoted = quoted.replace(character, '\\' + character)
    quoted = quoted.replace('\\', '\\\\')
    # A direct path also works in launchers with a stale or incomplete theme cache.
    launcher_icon = str(data / f"icons/hicolor/256x256/apps/{APP_ID}.png")
    launcher_icon = launcher_icon.replace("\\", "\\\\")
    desktop = data / f"applications/{APP_ID}.desktop"
    with tempfile.TemporaryDirectory(prefix="thinkhub-launcher-") as temporary:
        source = Path(temporary) / "launcher.desktop"
        source.write_text(
            "[Desktop Entry]\nType=Application\nName=ThinkHub\n"
            "Comment=ThinkPad hardware, battery health and controls\n"
            f'Exec="{quoted}"\nIcon={launcher_icon}\n'
            "Terminal=false\nCategories=Settings;HardwareSettings;\n"
            "Keywords=ThinkPad;Lenovo;battery;power;TrackPoint;\n"
            "StartupNotify=true\n", encoding="utf-8"
        )
        install_file(source, desktop, 0o644)
    for tool, command_args in [
        ("update-desktop-database", [str(desktop.parent)]),
        ("gtk-update-icon-cache", ["--ignore-theme-index", "--force", str(data / "icons/hicolor")]),
    ]:
        program = shutil.which(tool)
        if program is None and tool == "gtk-update-icon-cache":
            program = shutil.which("gtk4-update-icon-cache")
        if program:
            subprocess.run([program, *command_args], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    print(f"Installed ThinkHub: {target}\nLauncher: {desktop}")
    if not Path("/usr/local/bin/thinkpad-dashboard").is_file():
        print("The matching native backend must be installed separately (scripts/install-local.sh).")


if __name__ == "__main__":
    main()
