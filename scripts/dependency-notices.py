#!/usr/bin/env python3
"""Collect license texts for Linux dependency graphs from the locked crate sources."""
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]
packages={}
for manifest in ('Cargo.toml','gui/Cargo.toml'):
    data=json.loads(subprocess.check_output(['cargo','metadata','--manifest-path',manifest,
        '--locked','--format-version','1','--filter-platform','x86_64-unknown-linux-gnu'],cwd=ROOT,text=True))
    nodes={p['id']:p for p in data['resolve']['nodes']}
    seen=set()
    def visit(identifier):
        if identifier in seen:return
        seen.add(identifier)
        for dep in nodes[identifier]['deps']:visit(dep['pkg'])
    visit(data['resolve']['root'])
    for package in data['packages']:
        if package['id'] in seen and package.get('source'):
            packages[(package['name'],package['version'])]=package
output=['ThinkHub — third-party Rust dependency notices','',
        'Linux x86_64 dependency graphs from both lockfiles, including build and test dependencies.',
        'Not all listed crates are linked into every binary. Other target dependencies remain in Cargo.lock.',
        'GTK4 and other shared system libraries are supplied separately by the distro under their own licenses.','']
for (name,version),package in sorted(packages.items()):
    root=Path(package['manifest_path']).parent
    files=sorted(p for p in root.iterdir() if p.is_file() and p.name.upper().startswith(('LICENSE','COPYING','NOTICE')))
    if not files:raise SystemExit(f'Missing license text for {name} {version}; review before distribution.')
    output+=['='*72,f'{name} {version}',f'Declared license: {package.get("license")}',
             f'Source: https://crates.io/crates/{name}/{version}','']
    for path in files:output += [f'--- {path.name} ---',path.read_text(errors='replace').strip(),'']
(ROOT/'THIRD_PARTY_NOTICES.txt').write_text('\n'.join(output)+'\n')
print(f'Collected notices for {len(packages)} Linux dependency versions.')
