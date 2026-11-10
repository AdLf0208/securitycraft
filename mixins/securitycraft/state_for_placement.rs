// `native_state_for_placement` (API 164): the keypad faces its placer.
fn observe_state_for_placement(
    game: &crate::game::GameLevel<'_>,
    state: horizon_world::registry::BlockStateId,
    context: &horizon_world::level::PlacementContext,
    answer: &mut Option<Option<horizon_world::registry::BlockStateId>>,
) {
    crate::game::securitycraft_state::on_state_for_placement(game, state, context, answer);
}
