// Game-side glue for the SecurityCraft slice: the keypad block entity, its
// screen and menu buttons, the redstone it gives, and the harness commands.
//
// Injected into `crate::game` as `securitycraft_state`; it needs the game, so
// it is not part of the standalone library (only `runtime.rs` is).
//
// The keypad's whole state lives in its block entity's saved compound (`code`,
// `salt`, `mode`, `entry`, `status`, `signal`), so a save and a load keep it,
// as upstream's `KeypadBlockEntity.saveAdditional` does. A correct code sets
// the block's `powered` state for `DEFAULT_SIGNAL_TICKS`; `native_block_signal`
// answers 15 while it is set, which is the whole "link": a reinforced door
// beside the keypad is a vanilla door whose `neighborChanged` reads that
// signal, exactly as upstream's `BlockUtils.hasActiveSCBlockNextTo` reads a
// powered keypad or a 15 signal around the door.
//
// The reinforced door itself has no code: it is the carried `minecraft:iron_door`
// template (open only by redstone) with the jar's textures and loot.

use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer};
use horizon_client::hud::HudBatch;
use horizon_client::text::Text;
use horizon_core::random::RandomSource;
use horizon_world::level::PlacementContext;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;

use crate::game::Game;
use crate::game::GameLevel;
use crate::game::native_hooks::NativeScreen;
use crate::game::native_server::{NativeCommand, NativeServerPlayer};
use crate::game::securitycraft::{
    Action, Button, DEFAULT_SIGNAL_TICKS, Entry, Mode, after_submit, button, facing_from_yaw,
    hash_code, new_salt, tick_signal, valid_code,
};

/// The keypad block; its item (`securitycraft:keypad_item`) places it.
pub const KEYPAD: &str = "securitycraft:keypad";
/// The reinforced door block, whose item is the block's own (the double-high
/// placement of `minecraft:iron_door`).
pub const DOOR: &str = "securitycraft:iron_door_reinforced";
/// The keypad's menu type (`data/securitycraft/horizon/menu/keypad.json`).
pub const KEYPAD_MENU: &str = "securitycraft:keypad";
/// The panel the keypad screen draws, and where the player's inventory starts.
pub const SCREEN_WIDTH: f32 = 176.0;
pub const SCREEN_HEIGHT: f32 = 226.0;
pub const INVENTORY_Y: f32 = 140.0;

/// The saved keys, named after the mod's own fields.
const KEY_CODE: &str = "code";
const KEY_SALT: &str = "salt";
const KEY_MODE: &str = "mode";
const KEY_ENTRY: &str = "entry";
const KEY_STATUS: &str = "status";
const KEY_SIGNAL: &str = "signal";

/// What the screen shows after the last key press (`status`).
const STATUS_NONE: i32 = 0;
const STATUS_WRONG: i32 = 1;
const STATUS_SET: i32 = 2;
const STATUS_OPENED: i32 = 3;
const STATUS_EMPTY: i32 = 4;

/// The screen's button grid: 1-9, then backspace, 0 and submit. The same
/// rectangles are hit-tested in `native_screen_click` and drawn in
/// `native_screen_draw`, both relative to the panel's top left.
const GRID_X: f32 = 52.0;
const GRID_Y: f32 = 46.0;
const BUTTON_W: f32 = 20.0;
const BUTTON_H: f32 = 18.0;
const GAP_X: f32 = 26.0;
const GAP_Y: f32 = 22.0;

/// The rectangle of key `id` relative to the panel's top left, `None` for an
/// id the screen has no key for.
fn key_rect(id: i32) -> Option<[f32; 4]> {
    let (row, column) = match id {
        1..=9 => {
            let index = u32::try_from(id).ok()? - 1;
            (f32::from(i16::try_from(index / 3).ok()?), f32::from(i16::try_from(index % 3).ok()?))
        }
        // The bottom row: backspace, 0, submit.
        10 => (3.0, 0.0),
        0 => (3.0, 1.0),
        11 => (3.0, 2.0),
        _ => return None,
    };
    Some([
        GRID_X + column * GAP_X,
        GRID_Y + row * GAP_Y,
        BUTTON_W,
        BUTTON_H,
    ])
}

fn int(nbt: &Compound, key: &str) -> i32 {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

fn set_int(nbt: &mut Compound, key: &str, value: i32) {
    nbt.insert(JavaString::from(key), Tag::Int(value));
}

fn string(nbt: &Compound, key: &str) -> Option<String> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

fn set_string(nbt: &mut Compound, key: &str, value: &str) {
    nbt.insert(
        JavaString::from(key),
        Tag::String(JavaString::from(value)),
    );
}

fn has(nbt: &Compound, key: &str) -> bool {
    nbt.contains_key(&JavaString::from(key))
}

/// The block name at `pos`, empty when the level has nothing there.
fn block_at(game: &Game<'_>, pos: [i32; 3]) -> String {
    let state = game.world.block(pos[0], pos[1], pos[2]);
    game.native_state_block(state).unwrap_or("").to_owned()
}

/// The `powered` state of the block at `pos`.
fn powered_at(game: &Game<'_>, pos: [i32; 3]) -> bool {
    let state = game.world.block(pos[0], pos[1], pos[2]);
    game.native_block_state_property(state, "powered") == Some("true")
}

/// Give `KEYPAD` its block entity (`native_block_entity`, API 15).
#[allow(private_interfaces)]
pub fn on_block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if block == KEYPAD {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

/// The keypad ticks: its signal countdown and its menu's data slots
/// (`native_block_entity_ticks`, API 15).
#[allow(private_interfaces)]
pub fn on_block_entity_ticks(block: &str, ticks: &mut bool) {
    if block == KEYPAD {
        *ticks = true;
    }
}

/// The keypad's one (hidden) container slot and its menu type
/// (`native_block_entity_container`, API 16/17). No face exposes the slot, so
/// no hopper can put anything into a keypad.
#[allow(private_interfaces)]
pub fn on_container(block: &str, container: &mut Option<ModContainer>) {
    if block == KEYPAD {
        *container = Some(ModContainer {
            slots: 1,
            faces: std::array::from_fn(|_| Vec::new()),
            inputs: Vec::new(),
            outputs: Vec::new(),
            menu: Some(KEYPAD_MENU.to_owned()),
        });
    }
}

/// The data slots the screen reads: mode, typed length, status, code-set
/// flag, powered flag and the signal left.
fn data_slots(game: &Game<'_>, pos: [i32; 3], nbt: &Compound) -> Vec<i32> {
    let typed = string(nbt, KEY_ENTRY).map_or(0, |entry| entry.len());
    vec![
        int(nbt, KEY_MODE),
        i32::try_from(typed).unwrap_or(i32::MAX),
        int(nbt, KEY_STATUS),
        if has(nbt, KEY_CODE) { 1 } else { 0 },
        if powered_at(game, pos) { 1 } else { 0 },
        int(nbt, KEY_SIGNAL),
    ]
}

/// One keypad tick: count the signal down and turn it off when it reaches
/// zero, then keep the menu's data slots current (API 17, `block_entity_tick`).
#[allow(private_interfaces)]
pub fn on_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if entity.id != KEYPAD {
        return;
    }
    let signal = int(&entity.nbt, KEY_SIGNAL);
    if signal > 0 {
        let (next, turns_off) = tick_signal(signal);
        set_int(&mut entity.nbt, KEY_SIGNAL, next);
        if turns_off {
            let _ = game.native_set_block_property(pos, "powered", "false");
        }
    }
    let data = data_slots(game, pos, &entity.nbt);
    if entity.data != data {
        entity.data = data;
    }
}

/// A player uses the keypad (`native_right_click_block`, API 21): a powered
/// keypad does nothing (upstream's `POWERED` check), an open one starts the
/// check or set screen. The barrel template's own use opens the menu.
#[allow(private_interfaces)]
pub fn on_right_click_block(
    game: &mut Game<'_>,
    pos: [i32; 3],
    _face: [i32; 3],
    canceled: &mut bool,
) {
    if block_at(game, pos) != KEYPAD {
        return;
    }
    if powered_at(game, pos) {
        *canceled = true;
        return;
    }
    let _ = game.native_with_mod_block_entity(pos, |entity| {
        let mode = if has(&entity.nbt, KEY_CODE) {
            Mode::Check
        } else {
            Mode::Set
        };
        set_int(&mut entity.nbt, KEY_MODE, mode.code());
        set_string(&mut entity.nbt, KEY_ENTRY, "");
        set_int(&mut entity.nbt, KEY_STATUS, STATUS_NONE);
        entity.data = vec![
            mode.code(),
            0,
            STATUS_NONE,
            if mode == Mode::Check { 1 } else { 0 },
            0,
            int(&entity.nbt, KEY_SIGNAL),
        ];
    });
}

/// The keypad's screen layout (`native_screen`, API 19). The container's one
/// slot is left out: a keypad has no item view.
#[allow(private_interfaces)]
pub fn on_screen(block: &str, screen: &mut Option<NativeScreen>) {
    if block != KEYPAD {
        return;
    }
    *screen = Some(NativeScreen {
        width: SCREEN_WIDTH,
        height: SCREEN_HEIGHT,
        slots: vec![None],
        inventory_y: INVENTORY_Y,
        inventory_x: 8.0,
        own_labels: false,
        own_background: false,
    });
}

fn centred_text(
    game: &Game<'_>,
    batch: &mut HudBatch,
    origin: [f32; 2],
    text: &str,
    row: f32,
    rgb: u32,
) {
    let width = game.native_text_width(text);
    let x = origin[0] + (SCREEN_WIDTH - width) / 2.0;
    game.native_text(batch, text, [x, origin[1] + row], rgb, false);
}

/// Draws the code display and the twelve keys (`native_screen_draw`, API 19).
#[allow(private_interfaces)]
pub fn on_screen_draw(
    game: &Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if block != KEYPAD {
        return;
    }
    let mode = Mode::from_int(data.first().copied().unwrap_or(0));
    let typed = usize::try_from(data.get(1).copied().unwrap_or(0)).unwrap_or(0);
    let status = data.get(2).copied().unwrap_or(STATUS_NONE);
    let code_set = data.get(3).copied().unwrap_or(0) != 0;
    let (line, colour) = match status {
        STATUS_WRONG => ("Wrong code", 0x8b0000),
        STATUS_SET => ("Code set", 0x006400),
        STATUS_OPENED => ("Door opened", 0x006400),
        STATUS_EMPTY => ("Type a code first", 0x8b0000),
        _ if mode == Mode::Set => ("Set a code", 0x404040),
        _ => ("Enter the code", 0x404040),
    };
    centred_text(game, batch, origin, line, 8.0, colour);
    centred_text(game, batch, origin, &"*".repeat(typed), 24.0, 0x202020);
    for id in 0..=11 {
        let Some([x, y, width, height]) = key_rect(id) else {
            continue;
        };
        let at = [origin[0] + x, origin[1] + y];
        game.native_fill(batch, [at[0], at[1], width, height], 0xff9a9a9a);
        let label = match id {
            10 => "<".to_owned(),
            11 => "OK".to_owned(),
            digit => digit.to_string(),
        };
        let text_width = game.native_text_width(&label);
        game.native_text(
            batch,
            &label,
            [
                at[0] + (width - text_width) / 2.0,
                at[1] + (height - 8.0) / 2.0,
            ],
            0x202020,
            false,
        );
    }
    centred_text(
        game,
        batch,
        origin,
        if code_set { "a code is set" } else { "no code set" },
        INVENTORY_Y - 14.0,
        0x606060,
    );
}

/// A press on the keypad screen names the menu button the game sends to the
/// server (`native_screen_click`, API 20).
#[allow(private_interfaces)]
pub fn on_screen_click(
    block: &str,
    _data: &[i32],
    at: [f32; 2],
    _right: bool,
    button_id: &mut Option<i32>,
) {
    if block != KEYPAD {
        return;
    }
    for id in 0..=11 {
        let Some([x, y, width, height]) = key_rect(id) else {
            continue;
        };
        if (x..x + width).contains(&at[0]) && (y..y + height).contains(&at[1]) {
            *button_id = Some(id);
            return;
        }
    }
}

/// A fresh 16-byte salt from the level's random, as upstream's
/// `PasscodeUtils.generateSalt` draws one.
fn fresh_salt(game: &mut Game<'_>) -> String {
    let dimension = game.native_dimension().to_owned();
    let mut bytes = [0u8; 16];
    if let Some(random) = game.native_level_random(&dimension) {
        for chunk in bytes.chunks_mut(4) {
            let draw = random.next_int_bound(i32::MAX).unwrap_or(0).unsigned_abs();
            chunk.copy_from_slice(&draw.to_le_bytes());
        }
    }
    new_salt(&bytes)
}

/// The typed entry a block entity holds.
fn entry_of(nbt: &Compound) -> Entry {
    let mut entry = Entry::new();
    for byte in string(nbt, KEY_ENTRY).unwrap_or_default().bytes() {
        if byte.is_ascii_digit() {
            entry.push(byte - b'0');
        }
    }
    entry
}

/// Keeps the typed entry in the block entity, dropping the key when it is
/// empty so a cleared screen leaves nothing behind.
fn store_entry(entity: &mut ModBlockEntity, entry: &Entry) {
    if entry.is_empty() {
        entity.nbt.remove(&JavaString::from(KEY_ENTRY));
    } else {
        set_string(&mut entity.nbt, KEY_ENTRY, entry.text());
    }
}

/// Applies one screen key to the block entity. `None` for a digit or a
/// backspace (the entry changed), `Some(action)` when Enter was pressed.
fn apply_key(
    game: &mut Game<'_>,
    entity: &mut ModBlockEntity,
    key: Button,
) -> Option<Action> {
    let mut entry = entry_of(&entity.nbt);
    match key {
        Button::Digit(digit) => {
            entry.push(digit);
            store_entry(entity, &entry);
            None
        }
        Button::Backspace => {
            entry.backspace();
            store_entry(entity, &entry);
            None
        }
        Button::Submit => {
            let mode = Mode::from_int(int(&entity.nbt, KEY_MODE));
            let salt = string(&entity.nbt, KEY_SALT).unwrap_or_default();
            let stored = string(&entity.nbt, KEY_CODE);
            let action = after_submit(mode, entry.text(), &salt, stored.as_deref());
            match action {
                Action::Open => {
                    entry.clear();
                    store_entry(entity, &entry);
                    set_int(&mut entity.nbt, KEY_STATUS, STATUS_OPENED);
                    set_int(&mut entity.nbt, KEY_MODE, Mode::Check.code());
                }
                Action::Wrong => {
                    entry.clear();
                    store_entry(entity, &entry);
                    set_int(&mut entity.nbt, KEY_STATUS, STATUS_WRONG);
                }
                Action::Empty => {
                    set_int(&mut entity.nbt, KEY_STATUS, STATUS_EMPTY);
                }
                Action::Set => {
                    let salt = fresh_salt(game);
                    let code = hash_code(entry.text(), &salt).unwrap_or_default();
                    set_string(&mut entity.nbt, KEY_SALT, &salt);
                    set_string(&mut entity.nbt, KEY_CODE, &code);
                    entry.clear();
                    store_entry(entity, &entry);
                    set_int(&mut entity.nbt, KEY_STATUS, STATUS_SET);
                    set_int(&mut entity.nbt, KEY_MODE, Mode::Check.code());
                }
                Action::Check => {
                    set_int(&mut entity.nbt, KEY_MODE, Mode::Set.code());
                    set_int(&mut entity.nbt, KEY_STATUS, STATUS_NONE);
                }
            }
            Some(action)
        }
    }
}

/// The server applies a key press (`native_menu_button`, API 20).
#[allow(private_interfaces)]
pub fn on_menu_button(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    id: i32,
) {
    if entity.id != KEYPAD {
        return;
    }
    let Some(key) = button(id) else {
        return;
    };
    if let Some(Action::Open) = apply_key(game, entity, key) {
        set_int(&mut entity.nbt, KEY_SIGNAL, DEFAULT_SIGNAL_TICKS);
        let _ = game.native_set_block_property(pos, "powered", "true");
    }
    entity.data = data_slots(game, pos, &entity.nbt);
}

/// `BlockState.isSignalSource`: every keypad state is one
/// (`native_is_signal_source`, API 38), so redstone dust connects to it.
#[allow(private_interfaces)]
pub fn on_is_signal_source(game: &GameLevel<'_>, state: BlockStateId, source: &mut bool) {
    if is_keypad(game, state) {
        *source = true;
    }
}

/// `BlockState.getSignal`/`getDirectSignal`: 15 on every face while powered
/// (`native_block_signal`, API 38), as upstream's `KeypadBlock.getSignal`.
#[allow(private_interfaces)]
pub fn on_block_signal(game: &GameLevel<'_>, state: BlockStateId, signal: &mut i32) {
    if is_keypad(game, state) && is_powered(game, state) {
        *signal = 15;
    }
}

fn is_keypad(game: &GameLevel<'_>, state: BlockStateId) -> bool {
    game.registry
        .state(state)
        .is_some_and(|def| def.name == KEYPAD)
}

fn is_powered(game: &GameLevel<'_>, state: BlockStateId) -> bool {
    game.registry
        .state(state)
        .and_then(|def| def.properties.get("powered"))
        .is_some_and(|value| value.as_str() == "true")
}

/// The keypad is placed facing the player who placed it
/// (`native_state_for_placement`, API 164).
#[allow(private_interfaces)]
pub fn on_state_for_placement(
    game: &GameLevel<'_>,
    state: BlockStateId,
    context: &PlacementContext,
    answer: &mut Option<Option<BlockStateId>>,
) {
    if !is_keypad(game, state) {
        return;
    }
    let facing = facing_from_yaw(context.yaw);
    if let Some(next) = game.native_state_with_property(state, "facing", facing) {
        *answer = Some(Some(next));
    }
}

// ---------------------------------------------------------------------------
// Harness commands
// ---------------------------------------------------------------------------

/// `/securitycraft ...` (`register_commands`, API 8). `info` reads a keypad and
/// the reinforced door beside it, `code` sets a code without the screen. They
/// exist because a headless harness cannot type into a text field; they are
/// recorded as a harness addition, not gameplay.
#[allow(private_interfaces)]
pub fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    commands.push(NativeCommand::new("securitycraft info <pos:block_pos>"));
    commands.push(NativeCommand::op(
        "securitycraft code <pos:block_pos> <code:string>",
        2,
    ));
}

fn parse_pos(words: &[&str]) -> Option<[i32; 3]> {
    Some([
        words.first()?.parse().ok()?,
        words.get(1)?.parse().ok()?,
        words.get(2)?.parse().ok()?,
    ])
}

/// A keypad's readout and the reinforced door beside it, as `/securitycraft
/// info` prints it.
fn keypad_info(game: &Game<'_>, pos: [i32; 3]) -> Option<String> {
    let entity = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if entity.id == KEYPAD => entity,
        _ => return None,
    };
    let mode = Mode::from_int(int(&entity.nbt, KEY_MODE));
    let entry = entry_of(&entity.nbt);
    let mut door = "none".to_owned();
    for [x, y, z] in [
        [pos[0] + 1, pos[1], pos[2]],
        [pos[0] - 1, pos[1], pos[2]],
        [pos[0], pos[1] + 1, pos[2]],
        [pos[0], pos[1] - 1, pos[2]],
        [pos[0], pos[1], pos[2] + 1],
        [pos[0], pos[1], pos[2] - 1],
    ] {
        let state = game.world.block(x, y, z);
        if game.native_state_block(state) != Some(DOOR) {
            continue;
        }
        let open = game.native_block_state_property(state, "open") == Some("true");
        door = format!("{x},{y},{z}:{}", if open { "open" } else { "closed" });
        break;
    }
    Some(format!(
        "securitycraft keypad {} {} {} code={} entered={} mode={} signal={} powered={} status={} door={door}",
        pos[0],
        pos[1],
        pos[2],
        if has(&entity.nbt, KEY_CODE) { "set" } else { "none" },
        entry.masked(),
        match mode {
            Mode::Check => "check",
            Mode::Set => "set",
        },
        int(&entity.nbt, KEY_SIGNAL),
        if powered_at(game, pos) { "true" } else { "false" },
        int(&entity.nbt, KEY_STATUS),
    ))
}

/// Runs `/securitycraft` (`perform_command`, API 8).
#[allow(private_interfaces)]
pub fn on_perform_command(
    game: &mut Game<'_>,
    _player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["securitycraft", "info", x, y, z] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal("Usage: /securitycraft info <x> <y> <z>"));
                return;
            };
            match keypad_info(game, pos) {
                Some(line) => game.native_feedback(Text::literal(line)),
                None => game.native_feedback(Text::literal("No keypad at that position.")),
            }
        }
        ["securitycraft", "code", x, y, z, code] => {
            *ran = true;
            let Some(pos) = parse_pos(&[*x, *y, *z]) else {
                game.native_feedback(Text::literal(
                    "Usage: /securitycraft code <x> <y> <z> <digits>",
                ));
                return;
            };
            if !valid_code(code) {
                game.native_feedback(Text::literal("A code is one to twenty digits."));
                return;
            }
            if block_at(game, pos) != KEYPAD {
                game.native_feedback(Text::literal("No keypad at that position."));
                return;
            }
            let salt = fresh_salt(game);
            let digest = hash_code(code, &salt).unwrap_or_default();
            let done = game.native_with_mod_block_entity(pos, |entity| {
                set_string(&mut entity.nbt, KEY_SALT, &salt);
                set_string(&mut entity.nbt, KEY_CODE, &digest);
                set_string(&mut entity.nbt, KEY_ENTRY, "");
                set_int(&mut entity.nbt, KEY_MODE, Mode::Check.code());
                set_int(&mut entity.nbt, KEY_STATUS, STATUS_SET);
                set_int(&mut entity.nbt, KEY_SIGNAL, 0);
            });
            match done {
                Some(()) => game.native_feedback(Text::literal("Code set.")),
                None => game.native_feedback(Text::literal("No keypad at that position.")),
            }
        }
        _ => {}
    }
}
