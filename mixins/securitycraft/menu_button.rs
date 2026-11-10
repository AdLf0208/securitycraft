// The server applies a keypad key press (`native_menu_button`, API 20).
fn observe_menu_button(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
    id: i32,
) {
    crate::game::securitycraft_state::on_menu_button(game, pos, entity, id);
}
