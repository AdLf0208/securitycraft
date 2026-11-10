// `native_block_signal` (API 38): a powered keypad gives 15 on every face.
fn observe_block_signal(
    game: &crate::game::GameLevel<'_>,
    state: horizon_world::registry::BlockStateId,
    signal: &mut i32,
) {
    crate::game::securitycraft_state::on_block_signal(game, state, signal);
}
