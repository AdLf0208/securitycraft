#!/usr/bin/env python3
"""Package content *and* native code as one `.hmod` — the format the pin's game reads.

`docs/api/mod-files.md` in the pin: "Both, in one `.hmod`: the two manifests name
the same mod id. Content entries are checked as content and left out of the
build." So the archive holds, in this order:

    horizon-mod.toml, assets/…, data/…        the content half (`Content`)
    horizon-native.json                        the native manifest (`Bundle`)
    mixins/<id>/horizon-mixin.toml, *.rs, …    the native half

Only files the readers accept are added: the content roots above, and under
`mixins/<id>/` the `.rs`/`.toml` files, `data/` folders and the crate text files
(`README*`, `LICENSE*`, …) the bundle rules name. Anything else refuses the file.

Fixed dates and modes, entries in a fixed order, so the same sources always give
the same bytes and the same SHA-256. `--check` rebuilds in memory and fails when
the file on disk is not exactly that build (a hand-edited or stale artifact), so
`notes.md`'s hash is re-provable from the tree.

    tools/package_unified.py                     # build securitycraft-unified.hmod
    tools/package_unified.py --check             # verify the file on disk
    tools/package_unified.py --out FILE          # somewhere else
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CONTENT = ROOT / 'content'
GAME = '26.3'
DEFAULT_OUT = ROOT / 'securitycraft-unified.hmod'
TIMESTAMP = (2026, 1, 1, 0, 0, 0)

CONTENT_PATHS = ('assets/', 'data/')
TEXT_FILES = ('README', 'LICENSE', 'LICENCE', 'COPYING', 'NOTICE')


def content_files():
    """`horizon-mod.toml`, `assets/**`, `data/**` from the content tree, sorted."""
    files = {'horizon-mod.toml': (CONTENT / 'horizon-mod.toml').read_bytes()}
    for top in CONTENT_PATHS:
        for path in sorted((CONTENT / top.rstrip('/')).rglob('*')):
            if path.is_file():
                files[path.relative_to(CONTENT).as_posix()] = path.read_bytes()
    return dict(sorted(files.items()))


def native_files(bundle, ident, version, compat):
    """`horizon-native.json` and the bundle's accepted files, `mixins/<id>/…`."""
    manifest = json.dumps(
        dict(schema=1, id=ident, version=version, game=GAME,
             horizon=compat['horizon_revision'], mixins=compat['horizon_mixins_revision']),
        indent=2, sort_keys=True) + '\n'
    files = {'horizon-native.json': manifest.encode()}

    def data(rest):
        # `mixins/<id>/data/…`, or a top-level crate folder's `data/…`.
        parts = rest.split('/')
        return len(parts) >= 2 and (parts[0] == 'data'
                                    or (len(parts) >= 3 and parts[1] == 'data'))

    for path in sorted(bundle.rglob('*')):
        if not path.is_file():
            continue
        rest = path.relative_to(bundle).as_posix()
        name = path.name
        shallow = len(rest.split('/')) <= 2
        if (path.suffix in ('.rs', '.toml') or data(rest)
                or (shallow and name.startswith(TEXT_FILES)) or name == '.gitignore'):
            files[path.relative_to(ROOT).as_posix()] = path.read_bytes()
        else:
            raise SystemExit(f'a native bundle cannot carry {rest}')
    return files


def build():
    compat = json.loads((ROOT / 'compatibility.json').read_text())
    manifests = sorted((ROOT / 'mixins').glob('*/horizon-mixin.toml'))
    if len(manifests) != 1:
        raise SystemExit('the unified .hmod holds one bundle: keep one mixins/<id>/horizon-mixin.toml')
    bundle = manifests[0].parent
    mixin = tomllib.loads(manifests[0].read_text())
    mod = mixin['mod']
    content = tomllib.loads((CONTENT / 'horizon-mod.toml').read_text())
    if bundle.name != mod['id'] or content['id'] != mod['id']:
        raise SystemExit('mixins/<id>/ and content/horizon-mod.toml name different mod ids')
    if content['version'] != mod['version']:
        raise SystemExit('the content and native manifests name different versions')
    if mixin.get('compatibility', {}).get('horizon_revision') != compat['horizon_revision']:
        raise SystemExit('horizon-mixin.toml and compatibility.json pin different Horizon revisions')

    content_entries = content_files()
    native_entries = native_files(bundle, mod['id'], mod['version'], compat)
    overlap = sorted(set(content_entries) & set(native_entries))
    if overlap:
        raise SystemExit(f'entries appear in both halves: {overlap}')

    # Content first, then the native manifest and the bundle (mod-files.md's order).
    files = dict(content_entries)
    files.update(native_entries)
    out = ROOT / 'target' / f"{mod['id']}-unified.hmod.partial"
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in files.items():
            info = zipfile.ZipInfo(name, TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3  # Unix: the mode below means 0644.
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)
    bytes_ = out.read_bytes()
    out.unlink()
    return bytes_, len(content_entries), len(native_entries), mod


def describe(bytes_, content_count, native_count, mod):
    digest = hashlib.sha256(bytes_).hexdigest()
    with zipfile.ZipFile(io.BytesIO(bytes_)) as archive:
        names = archive.namelist()
    return dict(id=mod['id'], version=mod['version'],
                size=len(bytes_), sha256=digest, entries=len(names),
                content_entries=content_count, native_entries=native_count,
                native_manifest='horizon-native.json' in names,
                content_manifest='horizon-mod.toml' in names)


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--out', type=Path, default=DEFAULT_OUT)
    parser.add_argument('--check', action='store_true',
                        help='verify the file on disk is exactly a fresh build')
    args = parser.parse_args()

    bytes_, content_count, native_count, mod = build()
    report = describe(bytes_, content_count, native_count, mod)
    report['file'] = str(args.out)
    if args.check:
        if not args.out.is_file():
            raise SystemExit(f'missing {args.out}: build it with tools/package_unified.py')
        if args.out.read_bytes() != bytes_:
            raise SystemExit(f'{args.out} is not a fresh build of this tree '
                             f'(rebuild it: tools/package_unified.py --out {args.out})')
        report['matches_tree'] = True
    else:
        args.out.write_bytes(bytes_)
        report['matches_tree'] = True
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
