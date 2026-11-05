// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// RILEX SKINS — the mod's own crate. The helpers (command.rs, tick.rs, list.rs) only call
// `run` and `jobs::drain`; everything else lives here, where `cargo test` can reach it.
//
// Files in the mod's data folder (horizon-mods/.horizon-mod-data/rilex_skins/):
//   identity.pk8          the private key: whoever holds it owns the name
//   identity.pending.pk8  a new key waiting for the admin's approval (recovery)
//   name.txt              the registered name
//   api.txt               the registry's address, when not the built-in one
//   recovery-code.txt     the code shown at registration: copy it somewhere safe, then delete it

pub mod api;
pub mod commands;
pub mod jobs;
pub mod keys;
pub mod proto;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use commands::Cmd;
use keys::{Identity, KEY_FILE};
use std::path::{Path, PathBuf};

pub const ID: &str = "rilex_skins";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Put your Worker's address here before building, or set it in game with /rilexskin api <url>.
pub const DEFAULT_API: &str = "https://rilex-skins.YOUR-SUBDOMAIN.workers.dev";

const PENDING_FILE: &str = "identity.pending.pk8";
const P: &str = "[RILEX SKINS]";

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn read_trim(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn api_base(dir: &Path) -> String {
    read_trim(dir, "api.txt").unwrap_or_else(|| DEFAULT_API.to_string())
}

/// Runs a command. Quick answers come back at once; network work answers later through
/// `jobs::drain`, which the tick prints.
pub fn run(cmd: Cmd, dir: &Path) -> Vec<String> {
    let dir = dir.to_path_buf();
    match cmd {
        Cmd::Help => commands::HELP.iter().map(|s| s.to_string()).collect(),
        Cmd::Bad(msg) => vec![format!("{P} {msg}")],
        Cmd::Status => status(&dir),
        Cmd::Api(url) => match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("api.txt"), &url)) {
            Ok(()) => vec![format!("{P} skin server set to {url}")],
            Err(e) => vec![format!("{P} cannot save: {e}")],
        },
        Cmd::Register(name) => register(dir, name),
        Cmd::Set { path, model } => set_skin(dir, PathBuf::from(path), model),
        Cmd::Recover { name, code } => recover(dir, name, code),
        Cmd::Finish => finish(dir),
        Cmd::Backup => backup(&dir),
        Cmd::Restore(path) => restore(&dir, PathBuf::from(path)),
    }
}

fn status(dir: &Path) -> Vec<String> {
    let name = read_trim(dir, "name.txt");
    let key = Identity::load(dir, KEY_FILE).ok().flatten();
    let pending = dir.join(PENDING_FILE).exists();
    vec![
        format!("{P} v{VERSION} - server {}", api_base(dir)),
        format!("{P} name: {}", name.as_deref().unwrap_or("(not registered)")),
        format!("{P} key on this computer: {}", if key.is_some() { "yes" } else { "no" }),
        if pending { format!("{P} a recovery is waiting for the admin; then /rilexskin finish") } else { String::new() },
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect()
}

fn busy() -> Vec<String> {
    vec![format!("{P} still working on the last command, wait a moment")]
}

fn register(dir: PathBuf, name: String) -> Vec<String> {
    if !proto::valid_name(&name) {
        return vec![format!("{P} a name is 3-16 letters, digits or _")];
    }
    if let Some(have) = read_trim(&dir, "name.txt") {
        return vec![format!("{P} this computer is already registered as {have}")];
    }
    let id = match Identity::load(&dir, KEY_FILE) {
        Ok(Some(id)) => id,
        Ok(None) => match Identity::generate().and_then(|id| id.save(&dir, KEY_FILE).map(|_| id)) {
            Ok(id) => id,
            Err(e) => return vec![format!("{P} {e}")],
        },
        Err(e) => return vec![format!("{P} {e}")],
    };
    let code = match keys::new_recovery_code() {
        Ok(c) => c,
        Err(e) => return vec![format!("{P} {e}")],
    };
    let base = api_base(&dir);
    let started = jobs::spawn(move || {
        let lname = name.to_ascii_lowercase();
        let rh = keys::recovery_hash(&code);
        let ts = now();
        let sig = id.sign_b64(&proto::msg_register(&lname, &id.pubkey_b64(), &rh, ts));
        match api::Api::new(&base).register(&name, &id.pubkey_b64(), &rh, ts, &sig) {
            Ok(_) => {
                let _ = std::fs::write(dir.join("name.txt"), &name);
                let _ = std::fs::write(
                    dir.join("recovery-code.txt"),
                    format!("RILEX SKINS recovery code for {name}:\n{code}\n\nCopy it somewhere safe (paper, phone), then delete this file.\n"),
                );
                vec![
                    format!("{P} {name} is yours!"),
                    format!("{P} RECOVERY CODE: {code}"),
                    format!("{P} write it down - it is the only way back if you lose this computer"),
                    format!("{P} also saved in {}", dir.join("recovery-code.txt").display()),
                    format!("{P} next: /rilexskin set <path to skin.png>"),
                ]
            }
            Err(e) => vec![format!("{P} register failed: {e}")],
        }
    });
    if started { vec![format!("{P} registering...")] } else { busy() }
}

fn set_skin(dir: PathBuf, path: PathBuf, model: proto::Model) -> Vec<String> {
    let Some(name) = read_trim(&dir, "name.txt") else {
        return vec![format!("{P} register first: /rilexskin register <name>")];
    };
    let id = match Identity::load(&dir, KEY_FILE) {
        Ok(Some(id)) => id,
        Ok(None) => return vec![format!("{P} no key on this computer: /rilexskin restore or /rilexskin recover")],
        Err(e) => return vec![format!("{P} {e}")],
    };
    let png = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return vec![format!("{P} cannot open {}: {e}", path.display())],
    };
    if let Err(e) = proto::check_skin_png(&png) {
        return vec![format!("{P} {e}")];
    }
    let base = api_base(&dir);
    let started = jobs::spawn(move || {
        let lname = name.to_ascii_lowercase();
        let sha = keys::sha256_hex(&png);
        let ts = now();
        let sig = id.sign_b64(&proto::msg_skin(&lname, model, &sha, ts));
        match api::Api::new(&base).upload_skin(&name, model.as_str(), &B64.encode(&png), ts, &sig) {
            Ok(v) if v["unchanged"] == true => vec![format!("{P} that is already your skin")],
            Ok(_) => vec![format!("{P} skin saved ({}) - players with RILEX SKINS will see it", model.as_str())],
            Err(e) => vec![format!("{P} upload failed: {e}")],
        }
    });
    if started { vec![format!("{P} uploading your skin...")] } else { busy() }
}

fn recover(dir: PathBuf, name: String, code: String) -> Vec<String> {
    if !proto::valid_name(&name) {
        return vec![format!("{P} a name is 3-16 letters, digits or _")];
    }
    let new_id = match Identity::generate().and_then(|id| id.save(&dir, PENDING_FILE).map(|_| id)) {
        Ok(id) => id,
        Err(e) => return vec![format!("{P} {e}")],
    };
    let new_code = match keys::new_recovery_code() {
        Ok(c) => c,
        Err(e) => return vec![format!("{P} {e}")],
    };
    let base = api_base(&dir);
    let started = jobs::spawn(move || {
        let lname = name.to_ascii_lowercase();
        let new_rh = keys::recovery_hash(&new_code);
        let ts = now();
        let sig = new_id.sign_b64(&proto::msg_recover(&lname, &new_id.pubkey_b64(), &new_rh, ts));
        match api::Api::new(&base).recover(&name, &code, &new_id.pubkey_b64(), &new_rh, ts, &sig) {
            Ok(v) => {
                let _ = std::fs::write(dir.join("name.txt"), &name);
                let _ = std::fs::write(
                    dir.join("recovery-code.txt"),
                    format!("RILEX SKINS NEW recovery code for {name}:\n{new_code}\n\nThe old code no longer works.\n"),
                );
                let mut lines = vec![
                    format!("{P} code accepted"),
                    format!("{P} NEW RECOVERY CODE: {new_code} (the old one is dead) - write it down"),
                ];
                if v["status"] == "approved" {
                    lines.extend(finish_now(&dir));
                } else {
                    lines.push(format!("{P} waiting for the admin; when approved: /rilexskin finish"));
                }
                lines
            }
            Err(e) => {
                let _ = std::fs::remove_file(dir.join(PENDING_FILE));
                vec![format!("{P} recovery failed: {e}")]
            }
        }
    });
    if started { vec![format!("{P} checking your recovery code...")] } else { busy() }
}

/// After approval: the pending key becomes the key, once the registry shows it.
fn finish(dir: PathBuf) -> Vec<String> {
    if !dir.join(PENDING_FILE).exists() {
        return vec![format!("{P} nothing to finish")];
    }
    let started = jobs::spawn(move || finish_now(&dir));
    if started { vec![format!("{P} checking with the server...")] } else { busy() }
}

fn finish_now(dir: &Path) -> Vec<String> {
    let (Some(name), Ok(Some(pending))) = (read_trim(dir, "name.txt"), Identity::load(dir, PENDING_FILE)) else {
        return vec![format!("{P} nothing to finish")];
    };
    match api::Api::new(&api_base(dir)).profile(&name) {
        Ok(v) if v["profile"]["pubkey"].as_str() == Some(pending.pubkey_b64().as_str()) => {
            match std::fs::rename(dir.join(PENDING_FILE), dir.join(KEY_FILE)) {
                Ok(()) => vec![format!("{P} recovery done - {name} is yours again. Make a /rilexskin backup")],
                Err(e) => vec![format!("{P} cannot move the key: {e}")],
            }
        }
        Ok(_) => vec![format!("{P} not approved yet; try again later")],
        Err(e) => vec![format!("{P} {e}")],
    }
}

fn backup(dir: &Path) -> Vec<String> {
    let (Some(name), Ok(Some(id))) = (read_trim(dir, "name.txt"), Identity::load(dir, KEY_FILE)) else {
        return vec![format!("{P} nothing to back up yet")];
    };
    let file = dir.join(format!("backup-{name}.rilexkey"));
    let body = format!("RILEX SKINS key\nname={name}\nkey={}\n", id.export_b64());
    match std::fs::write(&file, body) {
        Ok(()) => vec![
            format!("{P} saved {}", file.display()),
            format!("{P} copy it to a USB stick or your phone. NEVER send it to anyone - it IS your name"),
        ],
        Err(e) => vec![format!("{P} cannot save: {e}")],
    }
}

fn restore(dir: &Path, path: PathBuf) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![format!("{P} cannot open {}", path.display())];
    };
    let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)).map(str::trim);
    let (Some(name), Some(key)) = (field("name="), field("key=")) else {
        return vec![format!("{P} this is not a RILEX SKINS backup")];
    };
    if !proto::valid_name(name) {
        return vec![format!("{P} this is not a RILEX SKINS backup")];
    }
    match Identity::import_b64(key).and_then(|id| id.save(dir, KEY_FILE)) {
        Ok(()) => match std::fs::write(dir.join("name.txt"), name) {
            Ok(()) => vec![format!("{P} restored - this computer is {name} now")],
            Err(e) => vec![format!("{P} {e}")],
        },
        Err(e) => vec![format!("{P} {e}")],
    }
}

pub const BRAND: &str = "RILEX MODS";

/// One line the first time the player is in a world this session.
pub fn welcome_once() -> Option<String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SHOWN: AtomicBool = AtomicBool::new(false);
    (!SHOWN.swap(true, Ordering::Relaxed)).then(|| format!("{P} by {BRAND} - your skin, offline. /rilexskin help"))
}
