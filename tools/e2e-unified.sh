#!/usr/bin/env bash
# Unified `.hmod` proof, in one build-slot lease (build-slot.sh e2e builds the
# native half from this port before running this script):
#
#   1. `tools/package_unified.py --check`: the file is exactly a fresh build of
#      `content/` + `horizon-native.json` + `mixins/securitycraft/`, and every
#      native entry in it is byte-identical to the sources this lease compiled.
#   2. the port's own scenario with `--dev-mod <unified .hmod>`: the content half
#      is read from the one file, the native half runs in the built game.
#   3. `SELFTEST=1`: the same scenario must fail (a scenario that cannot fail is
#      not evidence).
#   4. `CONTROL=1`: no package at all, the mod's ids must be unknown.
#
#   build-slot.sh e2e --job sec-unified --port ports/securitycraft -- tools/e2e-unified.sh
set -euo pipefail

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/securitycraft
EV=$CLONE/ports/_shared/evidence/securitycraft
UNIFIED=${UNIFIED:-$PORT/securitycraft-unified.hmod}
DRIVER_LOG=${DRIVER_LOG:-$EV/unified-driver.log}

mkdir -p "$(dirname "$DRIVER_LOG")" "$EV"
exec > >(tee "$DRIVER_LOG") 2>&1

[ -f "$UNIFIED" ] || { echo "missing $UNIFIED: run tools/package_unified.py" >&2; exit 1; }

echo "== 0/4 the unified .hmod is exactly a fresh build of this tree =="
python3 "$PORT/tools/package_unified.py" --check --out "$UNIFIED"

echo "== 0b/4 its native entries are the sources this lease compiles =="
python3 - "$UNIFIED" "$PORT" <<'PY'
import hashlib, sys, zipfile
from pathlib import Path
unified, port = Path(sys.argv[1]), Path(sys.argv[2])
with zipfile.ZipFile(unified) as archive:
    names = archive.namelist()
    assert 'horizon-mod.toml' in names and 'horizon-native.json' in names, names[:3]
    native = [n for n in names if n.startswith('mixins/')]
    assert native, 'no native half in the unified file'
    for name in native:
        source = port / name
        if not source.is_file():
            raise SystemExit(f'{name} is in the .hmod but not in the port')
        inside = archive.read(name)
        outside = source.read_bytes()
        if inside != outside:
            raise SystemExit(f'{name} differs between the .hmod and the port')
        print(f'  {name}  sha256={hashlib.sha256(inside).hexdigest()}  identical')
print(f'  {len(native)} native entries, all byte-identical to mixins/securitycraft/')
PY

echo "== 0c/4 the control binary is beside the game in this slot =="
if [ ! -x "$PORT/target/horizon-cargo/fast/horizon-ctl" ]; then
    echo "no horizon-ctl in this slot yet: building it"
    python3 "$PORT/tools/horizon.py" ctl
fi
[ -x "$PORT/target/horizon-cargo/fast/horizon-ctl" ] \
    || { echo "FAIL: horizon-ctl is not beside the game" >&2; exit 1; }

echo "== 1/4 the scenario, content loaded from the unified .hmod =="
DEV_MOD="$UNIFIED" LOG="$EV/e2e-unified.log" "$PORT/tools/e2e.sh"
grep -q '^PASS: the key panel placed a keypad' "$EV/e2e-unified.log" \
    || { echo "FAIL: the unified run printed no PASS line" >&2; exit 1; }
grep -q '^EXIT:0$' "$EV/e2e-unified.log" \
    || { echo "FAIL: the unified run did not end EXIT:0" >&2; exit 1; }

echo "== 2/4 selftest: the same scenario must fail =="
set +e
DEV_MOD="$UNIFIED" SELFTEST=1 LOG="$EV/e2e-unified-selftest.log" "$PORT/tools/e2e.sh"
selftest_status=$?
set -e
[ "$selftest_status" -eq 1 ] || { echo "FAIL: the selftest leg exited $selftest_status, not 1" >&2; exit 1; }
grep -q 'SELFTEST: expected the door OPEN after a wrong code' "$EV/e2e-unified-selftest.log" \
    || { echo "FAIL: the selftest leg did not fail at the selftest assertion" >&2; exit 1; }
grep -q '^EXIT:1$' "$EV/e2e-unified-selftest.log" \
    || { echo "FAIL: the selftest log does not end EXIT:1" >&2; exit 1; }
echo "selftest leg exited 1 at its own assertion (expected)"

echo "== 3/4 control: no package, the ids must be unknown =="
CONTROL=1 LOG="$EV/e2e-unified-control.log" "$PORT/tools/e2e.sh"
grep -q '^PASS: without the package the game does not know' "$EV/e2e-unified-control.log" \
    || { echo "FAIL: the control run printed no PASS line" >&2; exit 1; }
grep -q '^EXIT:0$' "$EV/e2e-unified-control.log" \
    || { echo "FAIL: the control run did not end EXIT:0" >&2; exit 1; }

echo "DRIVER PASS: the unified .hmod verified against the tree, the keypad and door worked from it, the selftest failed as it must, and the no-package control did not know the ids"
