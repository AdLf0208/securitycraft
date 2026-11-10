# Licence notice — SecurityCraft

This port carries the original mod's assets and data, and rewrites its behaviour
in Rust (or, for a data-tier port, declares its blocks from the same data). The
original licence and copyright notices are reproduced verbatim in
[LICENSE](LICENSE).

## Where the licence was read

- The pack jar `securitycraft-1.10.2.1.jar` (kept at
  `_shared/carry-work/securitycraft/`) declares
  `license="MIT https://github.com/Geforce132/SecurityCraft/blob/master/LICENSE"`
  in `META-INF/neoforge.mods.toml` and carries no licence file.
- The MIT text is reproduced from that declaration; the copyright line uses the
  jar's own `authors` field (`Geforce, bl4ckscor3, Redstone_Dubstep, and
  ChainmailPickaxe`), because the jar ships no copyright line.

## The port

- Original: SecurityCraft 1.10.2.1 by Geforce and contributors — MIT.
- Licence: **MIT**
- Port to Horizon by AdLf0208: the Rust runtime, the Horizon declarations and the
  build/verification tooling in this repository.
- No Mojang file is included. The carried files are the mod's own assets and
  data; `data/minecraft/tags/…` files only add this mod's ids to vanilla tags.
