// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// Prints what background work finished (registration, upload, recovery).
fn rilex_skins_tick(game: &mut crate::game::Game<'_>) {
    // The window's game only: in single player the server thread ticks too.
    if !game.native_window() {
        return;
    }
    if game.native_player().is_some() {
        if let Some(line) = rilex_skins_engine::welcome_once() {
            game.native_chat(&line);
        }
    }
    for line in rilex_skins_engine::jobs::drain() {
        game.native_chat(&line);
    }
}
