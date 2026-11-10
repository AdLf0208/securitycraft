# SecurityCraft — Horizon port

A port of [SecurityCraft](https://github.com/Geforce132/SecurityCraft) by
Geforce132 for Horizon. This first slice brings the **keypad** and the
**reinforced iron door**:

- Place a keypad, open it and type a code with its on-screen keypad; the code is
  stored salted and hashed, never in plain text.
- The keypad powers a redstone signal for 60 ticks when the right code is
  entered, which opens a reinforced iron door placed beside it.
- A wrong code leaves the door closed; a keypad with no door powers but opens
  nothing; an empty code stores nothing; a bare hand cannot open the reinforced
  door.
- `/securitycraft info|code` for testing.

Original copyright and licence notices are kept in the port repository. Pinned
to Horizon 1d63e0a and horizon-mixins 5a64af93. Later slices cover the
owner/allowlist system, the modules, the reinforced-block family and the
remaining machines.

**Original:** [SecurityCraft](https://github.com/Geforce132/SecurityCraft) 1.10.2.1 by
Geforce132, **MIT**. The original
copyright and licence notices are kept in [LICENSE](LICENSE), and
[LICENSE-NOTICE.md](LICENSE-NOTICE.md) records where each was read from.

**Horizon:** a port for [Horizon](https://github.com/horizon-rust/horizon), Minecraft:
Java Edition 26.3 rewritten in Rust. The exact Horizon and horizon-mixins
revisions this port builds against are pinned in `compatibility.json`.

## Install

1. Take `securitycraft.hnmod` from this repository (or build it below).
2. Put it in the game folder's `horizon-native-mods` folder — the game folder is the one
   holding `options.txt`.
3. Native mods are always on once installed; the first install shows a
   trust notice, and the launcher builds the mod for your game.

To try it without installing, start the game with
`--dev-mod securitycraft.hnmod`.

## Build

The port is a native mod (`.hnmod`): carried content plus a Rust runtime and
source mixins.

```sh
cargo test --locked --offline                        # the pure runtime rules
python3 tools/horizon.py stage   --horizon <horizon-checkout>
python3 tools/horizon.py check   --horizon <horizon-checkout>
python3 tools/horizon.py test    --horizon <horizon-checkout>
python3 tools/horizon.py build   --horizon <horizon-checkout>   # rebuild the .hnmod
```

`tools/e2e.sh` is the in-game harness. It expects a campaign checkout
(`$CAMPAIGN_ROOT`), the shared pins under `ports/_shared/pins/`, and the
prepared 26.3 resources (`$HORIZON_RES`, default `.local/horizon-client/26.3`).

## Layout

`content/` is the shipped content; `mixins/securitycraft/` holds the declarations and the runtime; `tools/` builds and verifies the port.

This is an unofficial port: Horizon is not affiliated with the original author.
The licence above is the original's and travels with every copy; see
[LICENSE](LICENSE) and [LICENSE-NOTICE.md](LICENSE-NOTICE.md).
