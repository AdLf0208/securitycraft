// The keypad's code display and keys (`native_screen_draw`, API 19).
fn observe_screen_draw(
    game: &crate::game::Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut horizon_client::hud::HudBatch,
    origin: [f32; 2],
) {
    crate::game::securitycraft_state::on_screen_draw(game, block, data, batch, origin);
}
