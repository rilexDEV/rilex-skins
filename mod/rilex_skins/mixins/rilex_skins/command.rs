// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// /rilexskin ... : handled here, never sent to the server.
fn rilex_skins_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let Some(cmd) = rilex_skins_engine::commands::parse(command) else {
        return;
    };
    if !game.native_on(rilex_skins_engine::ID) {
        return;
    }
    *ran = true;
    let dir = game.native_data_dir(rilex_skins_engine::ID);
    for line in rilex_skins_engine::run(cmd, &dir) {
        game.native_chat(&line);
    }
}
