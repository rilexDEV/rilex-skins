// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// The wire format shared with the registry (worker/src/index.js). Standard library only, so
// these rules can be tested without any crate. Change one side, change the other.

pub const CODE_PREFIX: &str = "RLX";

/// Minecraft player names: 3-16 letters, digits or `_`.
pub fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Classic,
    Slim,
}

impl Model {
    pub fn as_str(self) -> &'static str {
        match self {
            Model::Classic => "classic",
            Model::Slim => "slim",
        }
    }
    pub fn parse(s: &str) -> Option<Model> {
        match s.to_ascii_lowercase().as_str() {
            "classic" | "wide" | "steve" => Some(Model::Classic),
            "slim" | "alex" => Some(Model::Slim),
            _ => None,
        }
    }
}

pub fn msg_register(lname: &str, pubkey: &str, recovery_hash: &str, ts: u64) -> String {
    format!("rilex-skins/v1/register\n{lname}\n{pubkey}\n{recovery_hash}\n{ts}")
}

pub fn msg_skin(lname: &str, model: Model, skin_sha256: &str, ts: u64) -> String {
    format!("rilex-skins/v1/skin\n{lname}\n{}\n{skin_sha256}\n{ts}", model.as_str())
}

pub fn msg_recover(lname: &str, new_pubkey: &str, new_recovery_hash: &str, ts: u64) -> String {
    format!("rilex-skins/v1/recover\n{lname}\n{new_pubkey}\n{new_recovery_hash}\n{ts}")
}

/// What gets hashed for a recovery code: upper case, letters and digits only, so
/// "rlx aaaa-bbbb" and "RLX-AAAA-BBBB" are the same code.
pub fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

pub fn recovery_preimage(code: &str) -> String {
    format!("rilex-skins/v1/recovery\n{}", normalize_code(code))
}

/// Width and height from a PNG's IHDR, without decoding it.
pub fn png_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

pub const MAX_PNG_BYTES: usize = 24 * 1024;

/// The registry takes 64x64 skins (and old 64x32 ones), under 24 KB.
pub fn check_skin_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err(format!("the file is {} KB; a skin is under 24 KB", bytes.len() / 1024));
    }
    match png_dims(bytes) {
        None => Err("this is not a PNG file".into()),
        Some((64, 64)) | Some((64, 32)) => Ok(()),
        Some((w, h)) => Err(format!("the image is {w}x{h}; a skin is 64x64")),
    }
}

/// Group a code as RLX-XXXX-XXXX-XXXX-XXXX for reading out.
pub fn format_code(body: &str) -> String {
    let groups: Vec<String> = body.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
    format!("{CODE_PREFIX}-{}", groups.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_name("Rilex"));
        assert!(valid_name("a_b"));
        assert!(!valid_name("ab"));
        assert!(!valid_name("seventeen_chars_x"));
        assert!(!valid_name("no space"));
        assert!(!valid_name("فاروق"));
    }

    #[test]
    fn messages_match_the_worker() {
        // Same strings as worker/src/index.js builds; the worker test signs this exact message.
        assert_eq!(
            msg_register("rilex", "PK", "RH", 1762370000),
            "rilex-skins/v1/register\nrilex\nPK\nRH\n1762370000"
        );
        assert_eq!(msg_skin("rilex", Model::Slim, "S", 5), "rilex-skins/v1/skin\nrilex\nslim\nS\n5");
        assert_eq!(msg_recover("rilex", "N", "H", 7), "rilex-skins/v1/recover\nrilex\nN\nH\n7");
    }

    #[test]
    fn codes_normalize() {
        assert_eq!(normalize_code("rlx aaaa-bbbb"), "RLXAAAABBBB");
        assert_eq!(recovery_preimage("RLX-AAAA"), "rilex-skins/v1/recovery\nRLXAAAA");
        assert_eq!(format_code("ABCDEFGHJKMNPQRS"), "RLX-ABCD-EFGH-JKMN-PQRS");
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.resize(40, 0);
        b
    }

    #[test]
    fn skin_png_rules() {
        assert!(check_skin_png(&png(64, 64)).is_ok());
        assert!(check_skin_png(&png(64, 32)).is_ok());
        assert!(check_skin_png(&png(128, 128)).is_err());
        assert!(check_skin_png(b"hello").is_err());
        assert!(check_skin_png(&vec![0u8; 30_000]).is_err());
    }
}
