// The keypad's block entity (`native_block_entity`, API 15).
fn observe_block_entity(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    crate::game::securitycraft_state::on_block_entity(block, entity);
}
