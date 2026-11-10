#!/usr/bin/env bash
# In-game check for SecurityCraft slice 1: the keypad and the reinforced door.
#
# Upstream: a keypad stores a salted passcode; a player opens its screen,
# types the code and presses enter; a match cycles the keypad's POWERED state
# for `signalLength` ticks (60), and the reinforced door beside it opens on
# that redstone signal and closes when it ends. The port keeps the same shape:
# the code is a salted SHA-256 digest in the block entity, the screen is a drawn
# keypad whose keys arrive as menu buttons, and the door is the carried
# `minecraft:iron_door` template.
#
# Phases (Overworld, creative, a stone platform at y=100):
#  1. PLACE: the reinforced door item places both halves, the key panel places
#     the keypad; the door reads `open=false`.
#  2. SET: a fresh keypad opens in set mode; 1 2 3 4 + OK stores a code
#     (`code=set`, `mode=check`) and does not open anything.
#  3. WRONG: 9 9 9 9 + OK leaves the door closed (`status=1`, `powered=false`).
#  4. RIGHT: 1 2 3 4 + OK powers the keypad (`powered=true`) and the adjacent
#     reinforced door reads `open=true`.
#  5. CLOSE: after the signal length the keypad de-powers and the door closes.
#  6. UNLINKED (differential): a second keypad with the same code, no door
#     beside it, powers on the same right code, and the door stays closed.
#  7. NO CODE: a third, fresh keypad opens in set mode (`mode=set`) and OK with
#     nothing typed stores nothing (`code=none`, `status=4`).
#  8. WRONG BLOCK: an empty-hand click on the reinforced door opens no menu and
#     leaves it closed; so does a vanilla iron door (the control).
#
#   tools/e2e.sh               # the scenario above
#   SELFTEST=1 tools/e2e.sh    # phase 4 expects the door CLOSED: must exit 1
#   DEV_MOD=path/to/x.hmod tools/e2e.sh   # load a different content mod
#   CONTROL=1 tools/e2e.sh     # no package at all: the ids must be unknown
#
# The single-file deliverable (the AE2 pattern, `mixins/securitycraft/data/…`):
#   EMBEDDED=1 tools/e2e.sh    # launch with **no** `--dev-mod`: the built game
#                              # carries the bundle's own content; the run also
#                              # checks the file the helper wrote is byte-identical
#                              # to `content/securitycraft.hmod`
#   EMBEDDED=1 CONTROL=1 tools/e2e.sh     # the same build with `.horizon-cache`
#                              # unusable: the ids must be unknown (CONTROL
#                              # defaults to EMBEDDED for exactly this leg)
#
# Harness notes:
#  * the keypad screen has no visible slots, so the panel's origin comes from
#    `ctl ui`'s gui size: origin = ((w - 176) / 2, (h - 226) / 2);
#  * keys are hit-tested in `native_screen_click` and sent as menu buttons, so
#    `ctl click-at` at the key's centre is the whole input path;
#  * `/securitycraft info` is the harness readout (the task records it as such).
set -euo pipefail

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/securitycraft
CTL=${CTL:-$PORT/target/horizon-cargo/fast/horizon-ctl}
GAME=${GAME:-$PORT/target/horizon-cargo/fast/horizon-client}
RES=${RES:-${HORIZON_RES:-${HORIZON_RES:-$HOME/.local/horizon-client/26.3}}}
SOCK=${SOCK:-/tmp/opencode/securitycraft-e2e.sock}
SELFTEST=${SELFTEST:-0}
# A `.hmod` (or folder) to load with `--dev-mod`: the content half of the run.
DEV_MOD=${DEV_MOD:-$PORT/content}
# EMBEDDED=1: launch with no `--dev-mod`; the built game carries the bundle's own content.
EMBEDDED=${EMBEDDED:-0}
# CONTROL=1: prove the mod's ids are unknown. With EMBEDDED=1 that means the same build with
# the embedded content made unavailable (`.horizon-cache` a plain file), so the run proves the
# content came from the embedded copy; without it, no package at all is loaded.
CONTROL=${CONTROL:-0}
LOG=${LOG:-$CLONE/ports/_shared/evidence/securitycraft/e2e.log}

mkdir -p "$(dirname "$LOG")"
exec > >(tee "$LOG") 2>&1

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }
[ -f "$PORT/content/horizon-mod.toml" ] || { echo "missing content mod: $PORT/content" >&2; exit 1; }
command -v jq >/dev/null || { echo "missing jq" >&2; exit 1; }

KEYPAD=securitycraft:keypad
DOOR=securitycraft:iron_door_reinforced

SCRATCH=$(mktemp -d "${E2E_TMP:-/tmp/opencode}"/securitycraft-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"
rm -f "$SOCK"
cd "$SCRATCH"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
say() { echo "== $* =="; }
block_name() { ctl block "$1" "$2" "$3" | jq -r '.block // "unknown"'; }
block_state() { ctl block "$1" "$2" "$3" | jq -r '.state // "unknown"'; }
door_open() { block_state "$1" "$2" "$3" | sed -n 's/.*open=\(true\|false\).*/\1/p'; }
# /securitycraft info prints one chat line; the reply is the parser's.
info() { ctl chat "/securitycraft info $1 $2 $3" >/dev/null; ctl wait 6 >/dev/null; ctl messages 8 | jq -r '.chat[]' | grep -m1 '^securitycraft keypad ' || true; }
field() { sed -n "s/.*[ ]$1=\([^ ]*\).*/\1/p" <<<"$2"; }
code_cmd() { ctl chat "/securitycraft code $1 $2 $3 $4" >/dev/null; ctl wait 4 >/dev/null; }

# The keypad panel's top-left in GUI pixels, from the open screen's size.
panel_origin() {
    local ui w h
    ui=$(ctl ui)
    w=$(jq -r '.gui_size[0] | floor' <<<"$ui")
    h=$(jq -r '.gui_size[1] | floor' <<<"$ui")
    OX=$(( (w - 176) / 2 ))
    OY=$(( (h - 226) / 2 ))
}

open_screen() { # open_screen X Y Z: stand south of the keypad, then click it
    ctl close >/dev/null 2>&1 || true
    ctl tp "$1.5" "$2" "$(( $3 + 2 )).5" >/dev/null
    ctl wait 6
    ctl command "clear" >/dev/null
    ctl wait 2
    ctl look 180 0
    ctl wait 2
    ctl look-at block "$1" "$2" "$3" south >/dev/null 2>&1 || true
    ctl interact "$1" "$2" "$3" south >/dev/null || fail "the keypad did not accept the right click"
    ctl wait 6
    [ "$(ctl screen | jq -r '.screen')" = "container" ] || fail "the keypad screen did not open: $(ctl screen | jq -r '.screen')"
    panel_origin
}

press() { # press COLUMN ROW (0-based key grid) -> clicks the key's centre
    local x=$(( OX + 52 + $1 * 26 + 10 )) y=$(( OY + 46 + $2 * 22 + 9 ))
    ctl click-at "$x" "$y" >/dev/null || fail "the key press at $x,$y did not send"
    ctl wait 2
}

press_digit() {
    local digit=$1 row col
    if [ "$digit" = 0 ]; then row=3; col=1; else row=$(( (digit - 1) / 3 )); col=$(( (digit - 1) % 3 )); fi
    press "$col" "$row"
}
press_backspace() { press 0 3; }
press_submit() { press 2 3; }
type_code() { local digit; for digit in $(echo "$1" | grep -o .); do press_digit "$digit"; done; }

cleanup() {
    local status=$?
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    rm -rf "$SCRATCH" || true
    echo "EXIT:$status"
}
trap cleanup EXIT

# How the content half reaches the game: the `--dev-mod` content folder by default; with
# EMBEDDED=1 the built game carries the bundle's own copy (`mixins/<id>/data/…`, the AE2
# pattern) and is launched with no `--dev-mod`, as an installed `.hnmod` would be.
launch() { # launch WORLD
    rm -f "$SOCK"
    local args=(--game "$GAME" --world "$1" --seed 20261105 --creative --offscreen --wait
                --game-dir "$SCRATCH/game" --resources "$RES")
    if [ "$EMBEDDED" = 1 ]; then
        echo "  launch: no --dev-mod; the bundle carries its own content" >&2
    else
        args+=(--dev-mod "$DEV_MOD")
    fi
    "$CTL" --socket "$SOCK" launch "${args[@]}"
    ctl wait-ready 180
    if grep -q "content mods not loaded" "${SOCK}.log" 2>/dev/null; then
        grep "content mods not loaded" "${SOCK}.log" >&2
        fail "the content mod did not load"
    fi
}

# Fail-capability: with no content loaded the game must not know the mod's ids, so a PASS
# that resolved them cannot be vacuous.
ids_unknown() {
    local px pz reply block msgs
    say "control: the mod's item id is unknown"
    if reply=$(ctl give securitycraft:keypad_item 1 2>&1); then
        fail "the game gave an item from a package it did not load: $reply"
    fi
    echo "give: $reply"
    px=$(ctl state | jq -r '.player.position[0]' | cut -d. -f1)
    pz=$(ctl state | jq -r '.player.position[2]' | cut -d. -f1)
    px=$((px + 2)); pz=$((pz + 2))
    say "control: the mod's block id is unknown to setblock"
    # A refused setblock still replies ok:true; the refusal is the chat line.
    reply=$(ctl command "setblock $px 100 $pz securitycraft:keypad" 2>&1) || true
    echo "setblock: $reply"
    grep -q "Unknown block type 'securitycraft:keypad'" <<<"$reply" \
        || fail "setblock accepted a block from a package that was not loaded: $reply"
    block=$(block_name "$px" 100 "$pz")
    echo "block at $px,100,$pz: $block"
    [ "$block" != securitycraft:keypad ] || fail "an unloaded package's keypad is in the world"
    say "control: /securitycraft does not exist"
    ctl chat "/securitycraft info $px 100 $pz" >/dev/null 2>&1 || true
    ctl wait 6 >/dev/null
    msgs=$(ctl messages 8 | jq -r '.chat[]' 2>/dev/null || true)
    if grep -q '^securitycraft keypad ' <<<"$msgs"; then
        fail "the harness command answered without the package"
    fi
    echo "chat: $(grep -i 'securitycraft' <<<"$msgs" | head -1 || echo 'no /securitycraft reply')"
}

# EMBEDDED=1 CONTROL=1: the same built game (every hook is in it) with the embedded content
# made unavailable — `.horizon-cache` is a plain file, so the helper can neither compare nor
# write its content and falls back to Continue. If the ids resolve here, the embedded file
# proves nothing.
embedded_control_leg() {
    say "control: the same build with no embedded content file"
    rm -rf "$SCRATCH/.horizon-cache"
    : > "$SCRATCH/.horizon-cache"
    launch SecurityCraftEmbeddedControl
    ids_unknown
    [ -f "$SCRATCH/.horizon-cache" ] \
        || fail "the control's .horizon-cache was replaced: the helper wrote its content"
    [ ! -d "$SCRATCH/.horizon-cache" ] \
        || fail "the helper created a .horizon-cache directory in the control run"
    echo "  without the embedded file: no keypad item, the keypad block is unknown"
    ctl quit >/dev/null 2>&1 || true
    rm -f "$SCRATCH/.horizon-cache"
}

# EMBEDDED=1: the bundle must have written its content out at startup, byte for byte the
# delivered content mod.
embedded_content_check() {
    local cache=$SCRATCH/.horizon-cache/securitycraft-embedded-content.hmod
    [ -f "$cache" ] || fail "the bundle did not write its embedded content to $cache"
    cmp -s "$cache" "$PORT/content/securitycraft.hmod" \
        || fail "the embedded content differs from content/securitycraft.hmod"
    echo "  embedded content: $(wc -c < "$cache" | tr -d ' ') B at $cache, identical to content/securitycraft.hmod"
}

say "launch (offscreen renderer, creative, content + native bundle)"
if [ "$CONTROL" = 1 ]; then
    if [ "$EMBEDDED" = 1 ]; then
        embedded_control_leg
    else
        say "control: no package is loaded"
        launch SecurityCraftE2E
        ids_unknown
        echo "PASS: without the package the game does not know securitycraft:keypad_item or securitycraft:keypad, setblock refuses the block, no keypad exists at the target and /securitycraft answers nothing"
        exit 0
    fi
fi
launch SecurityCraftE2E
if [ "$EMBEDDED" = 1 ]; then embedded_content_check; fi

say "an Overworld platform at y=100, away from the terrain"
PX=$(ctl state | jq -r '.player.position[0]' | cut -d. -f1)
PZ=$(ctl state | jq -r '.player.position[2]' | cut -d. -f1)
ctl command "fill $((PX-8)) 100 $((PZ-8)) $((PX+8)) 100 $((PZ+8)) minecraft:stone" >/dev/null
ctl command "fill $((PX-8)) 101 $((PZ-8)) $((PX+8)) 110 $((PZ+8)) minecraft:air" >/dev/null
ctl tp "$((PX+1)).5" 101 "$((PZ+3)).5" >/dev/null
ctl wait 20
ctl command "gamerule spawn_mobs false" >/dev/null
ctl command "gamerule spawn_monsters false" >/dev/null
ctl command "kill @e[type=!minecraft:player]" >/dev/null
ctl wait 20
ctl close >/dev/null 2>&1 || true
ctl look 180 0
ctl wait 6

DOOR_X=$PX; DOOR_Z=$PZ
KEY_X=$((PX+1)); KEY_Y=101; KEY_Z=$PZ

say "phase 1: the reinforced door item places both halves, the key panel places a keypad"
ctl give "$DOOR" 2 >/dev/null
ctl give securitycraft:keypad_item 3 >/dev/null
ctl wait 6
ctl select "$DOOR" >/dev/null
ctl place "$DOOR_X" 101 "$DOOR_Z" >/dev/null || fail "the reinforced door item did not place"
ctl wait 4
block_name "$DOOR_X" 101 "$DOOR_Z" | grep -qx "$DOOR" || fail "no door lower half at $DOOR_X,101,$DOOR_Z"
block_name "$DOOR_X" 102 "$DOOR_Z" | grep -qx "$DOOR" || fail "no door upper half at $DOOR_X,102,$DOOR_Z"
echo "door: $(block_state "$DOOR_X" 101 "$DOOR_Z")"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = false ] || fail "the placed door is open"
ctl select securitycraft:keypad_item >/dev/null
ctl place "$KEY_X" "$KEY_Y" "$KEY_Z" >/dev/null || fail "the key panel did not place"
ctl wait 4
block_name "$KEY_X" "$KEY_Y" "$KEY_Z" | grep -qx "$KEYPAD" || fail "no keypad at $KEY_X,$KEY_Y,$KEY_Z"
echo "keypad: $(block_state "$KEY_X" "$KEY_Y" "$KEY_Z")"

say "phase 2: a fresh keypad opens in set mode; 1 2 3 4 + OK stores a code"
open_screen "$KEY_X" "$KEY_Y" "$KEY_Z"
type_code 1234
press_submit
line=$(info "$KEY_X" "$KEY_Y" "$KEY_Z")
echo "after set: $line"
[ "$(field code "$line")" = set ] || fail "the code was not stored: $line"
[ "$(field mode "$line")" = check ] || fail "the screen did not switch to check mode: $line"
[ "$(field powered "$line")" = false ] || fail "setting a code powered the keypad: $line"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = false ] || fail "setting a code opened the door"
ctl close >/dev/null 2>&1 || true
ctl wait 4

say "phase 3: a wrong code leaves the door closed"
open_screen "$KEY_X" "$KEY_Y" "$KEY_Z"
type_code 9999
press_submit
line=$(info "$KEY_X" "$KEY_Y" "$KEY_Z")
echo "after wrong code: $line"
[ "$(field status "$line")" = 1 ] || fail "the wrong code was not refused: $line"
[ "$(field powered "$line")" = false ] || fail "the wrong code powered the keypad: $line"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = false ] || fail "the wrong code opened the door"
ctl close >/dev/null 2>&1 || true
ctl wait 4
if [ "$SELFTEST" = 1 ]; then
    # Fail-capability: the same assertion must fail when it is wrong.
    [ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = true ] \
        || fail "SELFTEST: expected the door OPEN after a wrong code"
fi

say "phase 4: the right code powers the keypad and opens the adjacent reinforced door"
open_screen "$KEY_X" "$KEY_Y" "$KEY_Z"
type_code 1234
press_submit
line=$(info "$KEY_X" "$KEY_Y" "$KEY_Z")
echo "after right code: $line"
[ "$(field powered "$line")" = true ] || fail "the right code did not power the keypad: $line"
[ "$(field signal "$line")" -gt 0 ] || fail "the signal length was not started: $line"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = true ] || fail "the powered keypad did not open the door beside it"
ctl close >/dev/null 2>&1 || true

say "phase 5: the signal ends and the door closes"
ctl wait 70
line=$(info "$KEY_X" "$KEY_Y" "$KEY_Z")
echo "after the signal: $line"
[ "$(field powered "$line")" = false ] || fail "the keypad stayed powered: $line"
[ "$(field signal "$line")" = 0 ] || fail "the signal did not run out: $line"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = false ] || fail "the door stayed open after the signal ended"

say "phase 6 (differential): a keypad with the same code but no door powers on and opens nothing"
UNLINKED_X=$((PX+4)); UNLINKED_Z=$PZ
ctl close >/dev/null 2>&1 || true
ctl give securitycraft:keypad_item 1 >/dev/null
ctl wait 4
ctl select securitycraft:keypad_item >/dev/null
ctl place "$UNLINKED_X" "$KEY_Y" "$UNLINKED_Z" >/dev/null || fail "the second keypad did not place"
ctl wait 4
block_name "$UNLINKED_X" "$KEY_Y" "$UNLINKED_Z" | grep -qx "$KEYPAD" || fail "no second keypad"
code_cmd "$UNLINKED_X" "$KEY_Y" "$UNLINKED_Z" 1234
open_screen "$UNLINKED_X" "$KEY_Y" "$UNLINKED_Z"
type_code 1234
press_submit
line=$(info "$UNLINKED_X" "$KEY_Y" "$UNLINKED_Z")
echo "unlinked keypad: $line"
[ "$(field powered "$line")" = true ] || fail "the unlinked keypad did not power on: $line"
[ "$(field door "$line")" = none ] || fail "the unlinked keypad found a door: $line"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = false ] || fail "the unlinked keypad opened the first door"
ctl close >/dev/null 2>&1 || true
ctl wait 70

say "phase 7: a keypad with no code opens in set mode and OK with nothing typed stores nothing"
FRESH_X=$((PX+4)); FRESH_Z=$((PZ+4))
ctl give securitycraft:keypad_item 1 >/dev/null
ctl wait 4
ctl select securitycraft:keypad_item >/dev/null
ctl place "$FRESH_X" "$KEY_Y" "$FRESH_Z" >/dev/null || fail "the third keypad did not place"
ctl wait 4
open_screen "$FRESH_X" "$KEY_Y" "$FRESH_Z"
line=$(info "$FRESH_X" "$KEY_Y" "$FRESH_Z")
echo "fresh keypad: $line"
[ "$(field code "$line")" = none ] || fail "the fresh keypad has a code: $line"
[ "$(field mode "$line")" = set ] || fail "the fresh keypad did not open in set mode: $line"
press_submit
line=$(info "$FRESH_X" "$KEY_Y" "$FRESH_Z")
echo "after an empty submit: $line"
[ "$(field status "$line")" = 4 ] || fail "an empty submit was not refused: $line"
[ "$(field code "$line")" = none ] || fail "an empty submit stored a code: $line"
ctl close >/dev/null 2>&1 || true
ctl wait 4

say "phase 8 (control): the reinforced door does not open by hand"
ctl close >/dev/null 2>&1 || true
ctl tp "$((DOOR_X+1)).5" 101 "$((DOOR_Z+2)).5" >/dev/null
ctl wait 6
ctl look 180 0
ctl wait 2
ctl look-at block "$DOOR_X" 101 "$DOOR_Z" south >/dev/null 2>&1 || true
ctl interact "$DOOR_X" 101 "$DOOR_Z" south >/dev/null 2>&1 || true
ctl wait 4
[ "$(ctl screen | jq -r '.screen')" = "playing" ] || fail "a hand click on the reinforced door opened a screen"
[ "$(door_open "$DOOR_X" 101 "$DOOR_Z")" = false ] || fail "a hand click opened the reinforced door"
ctl close >/dev/null 2>&1 || true

echo "PASS: the key panel placed a keypad and the reinforced door item placed both halves; a fresh keypad set a code without opening anything; a wrong code left the reinforced door closed; the right code powered the keypad and opened the door beside it; the door closed when the signal ran out; a second keypad with the same code but no door powered on and opened nothing; an empty submit stored nothing; and a hand click cannot open the reinforced door"
# The cleanup trap prints the run's exit status as the last line.
