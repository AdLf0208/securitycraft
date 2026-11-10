// A press on a keypad key becomes a menu button (`native_screen_click`, API 20).
fn observe_screen_click(
    block: &str,
    data: &[i32],
    at: [f32; 2],
    right: bool,
    button: &mut Option<i32>,
) {
    crate::game::securitycraft_state::on_screen_click(block, data, at, right, button);
}
