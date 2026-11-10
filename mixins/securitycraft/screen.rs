// The keypad's panel layout (`native_screen`, API 19).
fn observe_screen(block: &str, screen: &mut Option<NativeScreen>) {
    crate::game::securitycraft_state::on_screen(block, screen);
}
