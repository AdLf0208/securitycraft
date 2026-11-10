#!/usr/bin/env bash
# Embedded-content proof: SecurityCraft as **one** `.hnmod`, the only file the registry stores.
#
# The bundle carries the content half (`mixins/securitycraft/data/securitycraft-content.hmod`,
# the built `content/securitycraft.hmod`) and writes it out from a `content_root` injection, so an
# install with no separate `.hmod` and no `--dev-mod` still has the keypad, the door and their
# behaviour. Steps, in one build-slot lease (`build-slot.sh e2e` builds the native half from this
# port first):
#
#   1. the delivered `.hnmod` carries `mixins/securitycraft/data/securitycraft-content.hmod`,
#      byte-identical to `content/securitycraft.hmod`, and every native entry is byte-identical
#      to the sources this lease compiled; `cargo horizon-mod check --kind native-mod` passes.
#   2. the port's own scenario with `EMBEDDED=1 CONTROL=1`: no `--dev-mod` at all, the content
#      comes from the embedded copy, the file the helper wrote is compared to the built one, and
#      the control (same build, `.horizon-cache` a plain file) shows the ids unknown.
#   3. `EMBEDDED=1 SELFTEST=1`: the same scenario must fail (a scenario that cannot fail is not
#      evidence).
#
#   build-slot.sh e2e --job sec-embed --port ports/securitycraft -- tools/e2e-embedded.sh
set -euo pipefail

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/securitycraft
EV=$CLONE/ports/_shared/evidence/securitycraft
HNMOD=${HNMOD:-$PORT/securitycraft.hnmod}
CONTENT=$PORT/content/securitycraft.hmod
DRIVER_LOG=${DRIVER_LOG:-$EV/e2e-embedded-driver.log}
HORIZON_MOD=${HORIZON_MOD:-$CLONE/ports/_shared/pins/horizon-1d63e0a/target/fast/cargo-horizon-mod}

mkdir -p "$EV"
exec > >(tee "$DRIVER_LOG") 2>&1

cleanup() {
    local status=$?
    echo "EXIT:$status"
}
trap cleanup EXIT

[ -f "$HNMOD" ] || { echo "missing $HNMOD: run tools/package.py and copy it to the port root" >&2; exit 1; }
[ -f "$CONTENT" ] || { echo "missing $CONTENT" >&2; exit 1; }

echo "== 1/3 the .hnmod carries its content, and it is the built content mod =="
cmp -s "$PORT/mixins/securitycraft/data/securitycraft-content.hmod" "$CONTENT" \
    || { echo "FAIL: mixins/securitycraft/data/securitycraft-content.hmod is not $CONTENT" >&2; exit 1; }
python3 - "$HNMOD" "$PORT" "$CONTENT" <<'PY'
import hashlib, sys, zipfile
from pathlib import Path
hnmod, port, content = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
mod = 'securitycraft'
with zipfile.ZipFile(hnmod) as archive:
    names = archive.namelist()
    embedded = f'mixins/{mod}/data/{mod}-content.hmod'
    assert embedded in names, f'{embedded} is not in the bundle: {names}'
    inside = archive.read(embedded)
    assert inside == content.read_bytes(), f'{embedded} differs from {content}'
    print(f'  {embedded}  {len(inside)} B  sha256={hashlib.sha256(inside).hexdigest()}  identical to content/')
    native = [n for n in names if n.startswith(f'mixins/{mod}/') and not n.endswith('.hmod')]
    assert native, 'no native entries in the bundle'
    for name in native:
        source = port / name
        if not source.is_file():
            raise SystemExit(f'{name} is in the .hnmod but not in the port')
        if archive.read(name) != source.read_bytes():
            raise SystemExit(f'{name} differs between the .hnmod and the port')
    content_entries = [n for n in names if not n.startswith('mixins/')]
    assert content_entries == ['horizon-native.json'], f'unexpected top-level entries: {content_entries}'
    print(f'  {len(native)} native entries byte-identical to mixins/{mod}/, plus the embedded content')
    print(f'  {len(names)} entries, {hnmod.stat().st_size} B in the file')
PY

echo "== 1b/3 cargo horizon-mod check --kind native-mod =="
"$HORIZON_MOD" check "$HNMOD" --kind native-mod

echo "== 2/3 the scenario with no --dev-mod, and its control =="
EMBEDDED=1 CONTROL=1 LOG="$EV/e2e-embedded.log" "$PORT/tools/e2e.sh"
grep -q '^  embedded content: .* identical to content/securitycraft.hmod$' "$EV/e2e-embedded.log" \
    || { echo "FAIL: the embedded run did not report the written content file" >&2; exit 1; }
grep -q 'without the embedded file: no keypad item' "$EV/e2e-embedded.log" \
    || { echo "FAIL: the control leg did not prove the ids unknown" >&2; exit 1; }
grep -q '^PASS: the key panel placed a keypad' "$EV/e2e-embedded.log" \
    || { echo "FAIL: the embedded run printed no PASS line" >&2; exit 1; }
grep -q '^EXIT:0$' "$EV/e2e-embedded.log" \
    || { echo "FAIL: the embedded run did not end EXIT:0" >&2; exit 1; }

echo "== 3/3 selftest: the same scenario must fail =="
set +e
EMBEDDED=1 SELFTEST=1 LOG="$EV/e2e-embedded-selftest.log" "$PORT/tools/e2e.sh"
selftest_status=$?
set -e
[ "$selftest_status" -eq 1 ] \
    || { echo "FAIL: the selftest leg exited $selftest_status, not 1" >&2; exit 1; }
grep -q 'SELFTEST: expected the door OPEN after a wrong code' "$EV/e2e-embedded-selftest.log" \
    || { echo "FAIL: the selftest leg did not fail at the selftest assertion" >&2; exit 1; }
grep -q '^EXIT:1$' "$EV/e2e-embedded-selftest.log" \
    || { echo "FAIL: the selftest log does not end EXIT:1" >&2; exit 1; }
echo "selftest leg exited 1 at its own assertion (expected)"

echo "DRIVER PASS: one .hnmod carried the content byte for byte, the keypad and reinforced door worked with no --dev-mod, the unusable-cache control did not know the ids, and the selftest failed as it must"
