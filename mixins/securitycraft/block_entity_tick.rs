// The keypad's signal countdown and menu data slots (`native_block_entity_tick`, API 17).
fn observe_block_entity_tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    crate::game::securitycraft_state::on_tick(game, pos, entity);
}
