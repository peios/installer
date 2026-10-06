import json, pathlib, sys, subprocess
root = pathlib.Path(sys.argv[1])
for binary in ['installer-gxwi', 'oobe-gxwi']:
    elf = root / 'usr/bin' / binary
    assert elf.is_file()
    dynamic = subprocess.check_output(['readelf', '-dW', elf], text=True)
    assert 'BIND_NOW' in dynamic and 'RELR' in dynamic
    assert 'RPATH' not in dynamic and 'RUNPATH' not in dynamic and 'TEXTREL' not in dynamic
    notes = subprocess.check_output(['readelf', '-n', elf], text=True)
    assert 'IBT' in notes and 'SHSTK' in notes
    build_id = notes.split('Build ID: ')[1].split()[0]
    debug = root / 'debug' / binary / 'usr/lib/debug/.build-id' / build_id[:2] / (build_id[2:] + '.debug')
    assert debug.is_file()
    assert (root / 'usr/share/man/man1' / (binary + '.1.gz')).is_file()
# The faces built into the pages, each with its licence beside it, which
# every package ships beside its own.
faces = sorted(p.name.removesuffix('.woff2') for p in pathlib.Path('page/fonts').glob('*.woff2'))
assert faces == ['jetbrains-mono', 'manrope', 'schibsted-grotesk'], faces
for face in faces:
    assert (pathlib.Path('page/fonts') / f'{face}.LICENSE').is_file(), face
for file in pathlib.Path('packages.pekit').glob('dev.*.toml'):
    assert '"@source:page/fonts/*.LICENSE"' in file.read_text(), file
# The one seed is the medium's overlay, for an image's live queue only.
seeds = sorted(p.name for p in (root / 'usr/share/regim').glob('*.reg'))
assert seeds == ['installer-gxwi-overlay.reg'], seeds
data = json.loads((root / 'usr/share/regim/installer-gxwi-overlay.reg').read_text())
values = {v['name']: v['data'] for k in data['keys'] for v in k['values']}
assert values == {'OverlayUsername': 'peios', 'OverlaySession': '/usr/bin/installer-gxwi'}, values
