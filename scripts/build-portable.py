#!/usr/bin/env python3
"""Build experimental static x86_64 backend binaries. No installation or publication."""
import argparse
import os
import pathlib
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--target-dir", type=pathlib.Path)
parser.add_argument("--offline", action="store_true")
args = parser.parse_args()
project = pathlib.Path(__file__).resolve().parent.parent
target_dir = (args.target_dir or project / "target/portable").resolve()
toolchain = "+1.85.0"
target = "x86_64-unknown-linux-musl"
sysroot = pathlib.Path(subprocess.check_output(["rustc", toolchain, "--print", "sysroot"], text=True).strip())
details = subprocess.check_output(["rustc", toolchain, "-vV"], text=True)
host = next(line.split(": ", 1)[1] for line in details.splitlines() if line.startswith("host: "))
linker = sysroot / "lib/rustlib" / host / "bin/rust-lld"
if not linker.is_file():
    raise SystemExit("Rust's bundled linker is missing; install the Rust 1.85.0 toolchain first.")
environment = dict(os.environ)
environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
    "-Ctarget-cpu=x86-64", "-Clink-self-contained=yes", "-Clinker=" + str(linker)
])
command = ["cargo", toolchain, "build", "--release", "--locked", "--target", target, "--target-dir", str(target_dir)]
if args.offline:
    command.append("--offline")
subprocess.run(command, cwd=project, env=environment, check=True)
for name in ["thinkpad-dashboard", "thinkpad-dashboard-helper"]:
    binary = target_dir / target / "release" / name
    headers = subprocess.check_output(["readelf", "-l", str(binary)], text=True, env=dict(os.environ, LC_ALL="C"))
    dynamic = subprocess.check_output(["readelf", "-d", str(binary)], text=True, env=dict(os.environ, LC_ALL="C"))
    if "INTERP" in headers or "(NEEDED)" in dynamic:
        raise SystemExit(f"Unexpected dynamic runtime dependency in {name}")
    print(binary)
