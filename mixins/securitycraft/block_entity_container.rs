// The keypad's one hidden slot and its menu type (`native_block_entity_container`, API 16).
fn observe_block_entity_container(
    block: &str,
    container: &mut Option<horizon_client::containers::ModContainer>,
) {
    crate::game::securitycraft_state::on_container(block, container);
}
