// The keypad ticks (`native_block_entity_ticks`, API 15).
fn observe_block_entity_ticks(block: &str, ticks: &mut bool) {
    crate::game::securitycraft_state::on_block_entity_ticks(block, ticks);
}
