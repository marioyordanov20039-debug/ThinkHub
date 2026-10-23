#!/usr/bin/env python3
"""Package already-built native binaries. Run on the target distro, without hardware writes."""
import argparse
import gzip
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
APP_ID = 'io.github.thinkhub.ThinkHub'
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
URL = 'https://github.com/marioyordanov20039-debug/ThinkHub'


def copy(source, root, destination, mode=0o644):
    target = root / destination.lstrip('/')
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    target.chmod(mode)


def stage(root):
    if b'/usr/bin/thinkpad-dashboard' not in (ROOT/'gui/target/release/thinkhub').read_bytes():
        raise SystemExit('Rebuild the GUI with THINKHUB_CLI_PATH=/usr/bin/thinkpad-dashboard before packaging.')
    for source, destination in [
        ('gui/target/release/thinkhub', '/usr/bin/thinkhub'),
        ('target/release/thinkpad-dashboard', '/usr/bin/thinkpad-dashboard'),
        ('target/release/thinkpad-dashboard-helper', '/usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper'),
    ]:
        copy(ROOT / source, root, destination, 0o755)
    copy(ROOT / 'data/io.github.thinkpaddashboard.control.policy', root,
         '/usr/share/polkit-1/actions/io.github.thinkpaddashboard.control.policy')
    copy(ROOT / 'LICENSE', root, '/usr/share/licenses/thinkhub/LICENSE')
    copy(ROOT / 'THIRD_PARTY_NOTICES.txt', root, '/usr/share/licenses/thinkhub/THIRD_PARTY_NOTICES.txt')
    for name in ('README.md', 'docs/INSTALL.md', 'docs/BIOS_UPDATES.md', 'docs/COMPATIBILITY.md'):
        copy(ROOT / name, root, '/usr/share/doc/thinkhub/' + Path(name).name)
    for icon in (ROOT / 'gui/assets/icons').glob('*/*.png'):
        copy(icon, root, f'/usr/share/icons/hicolor/{icon.parent.name}/apps/{icon.name}')
    copy(ROOT / f'gui/assets/{APP_ID}.svg', root,
         f'/usr/share/icons/hicolor/scalable/apps/{APP_ID}.svg')
    desktop = root / f'usr/share/applications/{APP_ID}.desktop'
    desktop.parent.mkdir(parents=True, exist_ok=True)
    desktop.write_text('[Desktop Entry]\nType=Application\nName=ThinkHub\n'
                       'Comment=ThinkPad hardware, battery health and controls\n'
                       'Exec=thinkhub\n'
                       f'Icon=/usr/share/icons/hicolor/256x256/apps/{APP_ID}.png\n'
                       'Terminal=false\nCategories=Settings;HardwareSettings;\n'
                       'Keywords=ThinkPad;Lenovo;battery;power;TrackPoint;\nStartupNotify=true\n')
    subprocess.run(['desktop-file-validate', str(desktop)], check=True)


def deb(root, out, baseline):
    # dpkg-shlibdeps derives actual minimum ABI versions from this distro's libraries.
    metadata = root / 'DEBIAN'
    metadata.mkdir()
    with tempfile.TemporaryDirectory(prefix='thinkhub-shlibs-') as tmp:
        cwd = Path(tmp)
        (cwd / 'debian').mkdir()
        (cwd / 'debian/control').write_text('Source: thinkhub\nSection: utils\nPriority: optional\n'
            'Maintainer: ThinkHub contributors <325435409+marioyordanov20039-debug@users.noreply.github.com>\n'
            'Standards-Version: 4.6.2\n\nPackage: thinkhub\nArchitecture: any\nDescription: ThinkPad dashboard\n')
        args = ['dpkg-shlibdeps', '-O'] + ['-e' + str(p) for p in [root/'usr/bin/thinkhub',
                root/'usr/bin/thinkpad-dashboard',root/'usr/lib/thinkpad-dashboard/thinkpad-dashboard-helper']]
        result = subprocess.check_output(args, cwd=cwd, text=True)
    dependencies = next(l.removeprefix('shlibs:Depends=') for l in result.splitlines()
                        if l.startswith('shlibs:Depends='))
    size = sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) // 1024 + 1
    (metadata / 'control').write_text(f'Package: thinkhub\nVersion: {VERSION}-1\nArchitecture: amd64\n'
        'Maintainer: ThinkHub contributors <325435409+marioyordanov20039-debug@users.noreply.github.com>\n'
        f'Installed-Size: {size}\nDepends: {dependencies}, pkexec | policykit-1\n'
        f'Recommends: fwupd\nSection: utils\nPriority: optional\nHomepage: {URL}\n'
        'Description: Native Rust ThinkPad dashboard for Linux\n'
        ' Hardware monitoring, capability-aware controls and appearance customization.\n'
        ' First preview; controls depend on the kernel, firmware and model.\n')
    subprocess.run(['dpkg-deb','--root-owner-group','--build',str(root),
                    str(out/f'thinkhub_{VERSION}-1_{baseline}_amd64.deb')],check=True)


def rpm(root, out, baseline, tmp):
    top = tmp / 'rpmbuild'
    for d in ('BUILD','BUILDROOT','RPMS','SOURCES','SPECS','SRPMS'):
        (top/d).mkdir(parents=True)
    filelist = '\n'.join('/' + str(p.relative_to(root)) for p in sorted(root.rglob('*')) if p.is_file())
    spec = top/'SPECS/thinkhub.spec'
    spec.write_text(f'''Name: thinkhub
Version: {VERSION}
Release: 1%{{?dist}}
Summary: Native Rust ThinkPad dashboard for Linux
License: MIT
URL: {URL}
Requires: polkit
Requires: libglvnd-gles
Recommends: fwupd
Recommends: mesa-dri-drivers
%description
Hardware monitoring, capability-aware controls and appearance customization.
First preview; controls depend on the kernel, firmware and model.
%prep
%build
%install
mkdir -p "%{{buildroot}}"
cp -a "{root}/." "%{{buildroot}}/"
%files
%defattr(-,root,root,-)
%dir /usr/lib/thinkpad-dashboard
%dir /usr/share/doc/thinkhub
%dir /usr/share/licenses/thinkhub
{filelist}
''')
    subprocess.run(['rpmbuild','--define',f'_topdir {top}', '--define','_build_id_links none',
                    '--define','debug_package %{nil}', '-bb',str(spec)],check=True)
    artifacts=list((top/'RPMS').rglob('*.rpm'))
    if len(artifacts)!=1:
        raise RuntimeError('Expected exactly one RPM')
    shutil.copyfile(artifacts[0],out/f'thinkhub-{VERSION}-1.{baseline}.x86_64.rpm')


def arch(root, out):
    files=[p for p in root.rglob('*') if p.is_file()]
    (root/'.PKGINFO').write_text(f'pkgname = thinkhub\npkgbase = thinkhub\npkgver = {VERSION}-1\n'
        'pkgdesc = Native Rust ThinkPad dashboard for Linux (preview)\n'
        f'url = {URL}\nbuilddate = {int(os.environ.get("SOURCE_DATE_EPOCH",time.time()))}\n'
        'packager = ThinkHub contributors\n'
        f'size = {sum(p.stat().st_size for p in files)}\narch = x86_64\nlicense = MIT\n'
        'depend = gtk4\ndepend = polkit\ndepend = glibc\ndepend = gcc-libs\n'
        'optdepend = fwupd: BIOS information and supported update preparation\n'
        'optdepend = systemd: busctl transport for power profiles\n')
    # Standard pacman package archive. Ownership is normalized even when built unprivileged.
    mtree = subprocess.check_output(['bsdtar', '--format=mtree', '--uid', '0', '--gid', '0',
        '--options=!all,use-set,type,uid,gid,mode,time,size,sha256,link',
        '-cf', '-', '-C', str(root), '.PKGINFO', 'usr'])
    (root/'.MTREE').write_bytes(gzip.compress(mtree, mtime=0))
    output=out/f'thinkhub-{VERSION}-1-x86_64.pkg.tar.zst'
    subprocess.run(['bsdtar','--uid','0','--gid','0','--uname','root','--gname','root',
                    '--zstd','-cf',str(output),'-C',str(root),'.PKGINFO','.MTREE','usr'],check=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--format',required=True,choices=('deb','rpm','arch'))
    parser.add_argument('--baseline',required=True,choices=('ubuntu22.04','debian12','fedora43','arch'))
    parser.add_argument('--output',type=Path,default=ROOT/'dist')
    args=parser.parse_args()
    compatible={'deb':{'ubuntu22.04','debian12'},'rpm':{'fedora43'},'arch':{'arch'}}
    if args.baseline not in compatible[args.format]:
        parser.error('Package format and distro baseline do not match')
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='thinkhub-package-') as temporary:
        tmp=Path(temporary);root=tmp/'root';root.mkdir()
        stage(root)
        if args.format=='deb':deb(root,out,args.baseline)
        elif args.format=='rpm':rpm(root,out,args.baseline,tmp)
        else:arch(root,out)


if __name__=='__main__':main()
