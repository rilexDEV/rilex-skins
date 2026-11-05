// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// The player's Ed25519 identity and recovery codes. Only ring, sha2 and base64, all in
// Horizon's Cargo.lock.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{Ed25519KeyPair, KeyPair};
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::proto;

pub const KEY_FILE: &str = "identity.pk8";

pub struct Identity {
    pkcs8: Vec<u8>,
    pair: Ed25519KeyPair,
}

impl Identity {
    pub fn generate() -> Result<Identity, String> {
        let rng = SystemRandom::new();
        let doc = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| "could not make a key".to_string())?;
        Identity::from_pkcs8(doc.as_ref().to_vec())
    }

    pub fn from_pkcs8(pkcs8: Vec<u8>) -> Result<Identity, String> {
        let pair = Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| "the key file is damaged".to_string())?;
        Ok(Identity { pkcs8, pair })
    }

    /// The key in `dir/<file>`, or None when there is none yet.
    pub fn load(dir: &Path, file: &str) -> Result<Option<Identity>, String> {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => Identity::from_pkcs8(bytes).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read the key: {e}")),
        }
    }

    /// Writes the key next to a temporary file first, so a crash never leaves half a key.
    pub fn save(&self, dir: &Path, file: &str) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tmp = dir.join(format!("{file}.tmp"));
        std::fs::write(&tmp, &self.pkcs8).map_err(|e| format!("cannot save the key: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, dir.join(file)).map_err(|e| format!("cannot save the key: {e}"))
    }

    pub fn pubkey_b64(&self) -> String {
        B64.encode(self.pair.public_key().as_ref())
    }

    pub fn sign_b64(&self, message: &str) -> String {
        B64.encode(self.pair.sign(message.as_bytes()).as_ref())
    }

    /// For the backup file: the private key, base64. Whoever holds it owns the name.
    pub fn export_b64(&self) -> String {
        B64.encode(&self.pkcs8)
    }

    pub fn import_b64(s: &str) -> Result<Identity, String> {
        let bytes = B64.decode(s.trim()).map_err(|_| "the backup is not valid".to_string())?;
        Identity::from_pkcs8(bytes)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn recovery_hash(code: &str) -> String {
    sha256_hex(proto::recovery_preimage(code).as_bytes())
}

/// RLX-XXXX-XXXX-XXXX-XXXX from 16 random characters of an alphabet without 0/O, 1/I/L.
/// 31^16 ≈ 2^79 possibilities; the registry also locks a name after 5 wrong tries.
pub fn new_recovery_code() -> Result<String, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let rng = SystemRandom::new();
    let mut out = String::with_capacity(16);
    while out.len() < 16 {
        let mut buf = [0u8; 32];
        rng.fill(&mut buf).map_err(|_| "no randomness available".to_string())?;
        for b in buf {
            // 248 = 31 * 8: dropping 248..=255 keeps every character equally likely.
            if b < 248 && out.len() < 16 {
                out.push(ALPHABET[(b % 31) as usize] as char);
            }
        }
    }
    Ok(proto::format_code(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 8032 test 1 key. The worker test (worker/test/worker.test.mjs) signs the same
    // message with WebCrypto and prints this signature: if both agree, Rust and the
    // registry speak the same protocol.
    const SEED: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";

    fn rfc_identity() -> Ed25519KeyPair {
        let seed: Vec<u8> = (0..32).map(|i| u8::from_str_radix(&SEED[i * 2..i * 2 + 2], 16).unwrap()).collect();
        Ed25519KeyPair::from_seed_unchecked(&seed).unwrap()
    }

    #[test]
    fn same_signature_as_the_worker() {
        let pair = rfc_identity();
        let pubkey = B64.encode(pair.public_key().as_ref());
        assert_eq!(pubkey, "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=");
        let rh = recovery_hash("RLX-AAAA-BBBB-CCCC-DDDD");
        assert_eq!(rh, "1f58381155a96b3fa759c17c31adeebb4f33bb2c9f488ca2129b03982386c182");
        let msg = proto::msg_register("rilex", &pubkey, &rh, 1762370000);
        let sig = B64.encode(pair.sign(msg.as_bytes()).as_ref());
        assert_eq!(
            sig,
            "WWX4m6xtKP1kBZ1IFkxehDQtAKbm8rJO5of6DcVED6oqvTcWNPyeaJRy7NBHe1KUTUdXa3Fr6LevLVoKxdRAAA=="
        );
    }

    #[test]
    fn key_round_trip() {
        let dir = std::env::temp_dir().join(format!("rilex-skins-test-{}", std::process::id()));
        let id = Identity::generate().unwrap();
        id.save(&dir, KEY_FILE).unwrap();
        let back = Identity::load(&dir, KEY_FILE).unwrap().unwrap();
        assert_eq!(back.pubkey_b64(), id.pubkey_b64());
        let restored = Identity::import_b64(&id.export_b64()).unwrap();
        assert_eq!(restored.pubkey_b64(), id.pubkey_b64());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_codes() {
        let a = new_recovery_code().unwrap();
        let b = new_recovery_code().unwrap();
        assert_ne!(a, b);
        assert_eq!(a.len(), "RLX-XXXX-XXXX-XXXX-XXXX".len());
        assert!(a.starts_with("RLX-"));
        assert!(!a[4..].contains(['0', 'O', '1', 'I', 'L']), "the prefix RLX is fixed; the body avoids look-alikes");
    }
}
