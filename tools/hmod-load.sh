#!/usr/bin/env bash
# Proves the built content package (content/securitycraft.hmod) loads on a real
# game: the shared, unmodded 1d63e0a5 game with `--dev-mod <the .hmod>` must
# register the keypad block, its key panel item and the reinforced door. A
# second launch WITHOUT `--dev-mod` is the negative control: the same ids must
# be unknown there, so the check cannot pass vacuously.
set -euo pipefail
export LC_ALL=C

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PIN_BIN=$CLONE/ports/_shared/bin/horizon-1d63e0a5
CTL=${CTL:-$PIN_BIN/horizon-ctl}
GAME=${GAME:-$PIN_BIN/horizon-client}
HMOD=${HMOD:-$CLONE/ports/securitycraft/content/securitycraft.hmod}
RES=${RES:-${HORIZON_RES:-${HORIZON_RES:-$HOME/.local/horizon-client/26.3}}}
SOCK=${SOCK:-/tmp/opencode/securitycraft-hmod.sock}
LOG=${LOG:-$CLONE/ports/_shared/evidence/securitycraft/hmod-load.log}

mkdir -p "$(dirname "$LOG")"
exec > >(tee "$LOG") 2>&1

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -f "$HMOD" ] || { echo "missing package: $HMOD" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }

SCRATCH=$(mktemp -d "${E2E_TMP:-/tmp/opencode}"/securitycraft-hmod-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")" "$XDG_CONFIG_HOME"
rm -f "$SOCK"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }

cleanup() {
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    if [ "${KEEP:-0}" = 1 ]; then echo "kept scratch: $SCRATCH" >&2; else rm -rf "$SCRATCH" 2>/dev/null || true; fi
}
trap cleanup EXIT

launch() { # launch WORLD [--dev-mod PATH]
    local world=$1; shift
    rm -f "$SOCK"
    cd "$SCRATCH"
    "$CTL" --socket "$SOCK" launch \
        --game "$GAME" --world "$world" --seed 20261105 --creative --headless --wait \
        --resources "$RES" "$@" >/dev/null
    ctl wait-ready 120
    ctl wait 20
}

echo "== launch WITH the package (--dev-mod $HMOD) =="
launch SecurityCraftHmod --dev-mod "$HMOD"
ctl command "setblock 0 64 0 securitycraft:keypad" >/dev/null
ctl wait 4
block=$(ctl block 0 64 0 | jq -r '.name // .block // empty')
echo "block 0,64,0 = $block"
[ "$block" = "securitycraft:keypad" ] || fail "the package did not register the keypad block"
ctl command "setblock 2 64 0 securitycraft:iron_door_reinforced[facing=south,half=lower,hinge=left,open=false]" >/dev/null
ctl wait 4
door=$(ctl block 2 64 0 | jq -r '.name // .block // empty')
echo "block 2,64,0 = $door"
[ "$door" = "securitycraft:iron_door_reinforced" ] || fail "the package did not register the reinforced door"
ctl give securitycraft:keypad_item 1 >/dev/null
ctl wait 4
ctl inventory | jq -e '[.hotbar[] | select(.item == "securitycraft:keypad_item")] | length >= 1' >/dev/null \
    || fail "the package did not register the key panel item: $(ctl inventory | jq -c '{hotbar,main}')"
echo "key panel item: $(ctl inventory | jq -c '[.hotbar[] | select(.item == "securitycraft:keypad_item")]')"
ctl quit >/dev/null 2>&1 || true
sleep 2

echo "== negative control: the SAME ids without --dev-mod =="
launch SecurityCraftHmodControl
out=$(ctl command "setblock 0 64 0 securitycraft:keypad")
echo "setblock reply: $out"
if grep -q 'Changed the block' <<<"$out"; then
    fail "the unmodded game accepted securitycraft:keypad (the check is vacuous)"
fi
block=$(ctl block 0 64 0 | jq -r '.name // .block // empty')
echo "control block 0,64,0 = $block"
[ "$block" != "securitycraft:keypad" ] || fail "the control block is the keypad"
ctl give securitycraft:keypad_item 1 >/dev/null || true
ctl wait 4
ctl inventory | jq -e '[.hotbar[] | select(.item == "securitycraft:keypad_item")] | length == 0' >/dev/null \
    || fail "the unmodded game gave a securitycraft item"

echo "PASS: securitycraft.hmod registered securitycraft:keypad, securitycraft:iron_door_reinforced and securitycraft:keypad_item on a real game, and the same ids are unknown without the package"
echo "EXIT:0"
