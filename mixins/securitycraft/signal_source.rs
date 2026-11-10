// `native_is_signal_source` (API 38): every keypad state is a signal source.
fn observe_is_signal_source(
    game: &crate::game::GameLevel<'_>,
    state: horizon_world::registry::BlockStateId,
    source: &mut bool,
) {
    crate::game::securitycraft_state::on_is_signal_source(game, state, source);
}
