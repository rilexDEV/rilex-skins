// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// The mod's row in Options > Mods.
fn rilex_skins_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: rilex_skins_engine::ID.into(),
        title: format!("RILEX SKINS {} - by {}", rilex_skins_engine::VERSION, rilex_skins_engine::BRAND),
        description: "A RILEX MODS mod. Your own skin, seen by every RILEX SKINS player. /rilexskin help".into(),
        keys: Vec::new(),
    });
}
