// `/securitycraft info|code` (`register_commands`, API 8).
fn observe_register_commands(commands: &mut Vec<NativeCommand>) {
    crate::game::securitycraft_state::on_register_commands(commands);
}
