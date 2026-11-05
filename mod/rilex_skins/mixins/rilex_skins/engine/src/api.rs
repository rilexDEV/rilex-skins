// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// HTTP calls to the registry. Blocking (ureq), so they only ever run on a background
// thread (see jobs.rs), never on the game's thread.

use serde_json::{Value, json};
use std::time::Duration;

pub struct Api {
    base: String,
    agent: ureq::Agent,
}

impl Api {
    pub fn new(base: &str) -> Api {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("RILEX-Skins-Mod/", env!("CARGO_PKG_VERSION")))
            .build();
        Api { base: base.trim_end_matches('/').to_string(), agent }
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        let url = format!("{}{}", self.base, path);
        let res = self.agent.post(&url).set("Content-Type", "application/json").send_string(&body.to_string());
        read(res)
    }

    pub fn get(&self, path: &str) -> Result<Value, String> {
        read(self.agent.get(&format!("{}{}", self.base, path)).call())
    }

    pub fn register(&self, name: &str, pubkey: &str, recovery_hash: &str, ts: u64, sig: &str) -> Result<Value, String> {
        self.post("/v1/register", json!({ "name": name, "pubkey": pubkey, "recovery_hash": recovery_hash, "ts": ts, "sig": sig }))
    }

    pub fn upload_skin(&self, name: &str, model: &str, png_b64: &str, ts: u64, sig: &str) -> Result<Value, String> {
        self.post("/v1/skin", json!({ "name": name, "model": model, "png": png_b64, "ts": ts, "sig": sig }))
    }

    pub fn recover(&self, name: &str, code: &str, new_pubkey: &str, new_rh: &str, ts: u64, sig: &str) -> Result<Value, String> {
        self.post(
            "/v1/recover",
            json!({ "name": name, "code": code, "new_pubkey": new_pubkey, "new_recovery_hash": new_rh, "ts": ts, "sig": sig }),
        )
    }

    pub fn profile(&self, name: &str) -> Result<Value, String> {
        self.get(&format!("/v1/skin/{name}"))
    }

    /// Up to 50 names in one call; what the skin renderer (phase 2) will use.
    pub fn profiles(&self, names: &[&str]) -> Result<Value, String> {
        self.get(&format!("/v1/skins?names={}", names.join(",")))
    }
}

/// The registry answers JSON for errors too: turn `{ error, message }` into a readable line.
fn read(res: Result<ureq::Response, ureq::Error>) -> Result<Value, String> {
    match res {
        Ok(r) => r.into_string().map_err(|e| e.to_string()).and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string())),
        Err(ureq::Error::Status(code, r)) => {
            let body: Value = r.into_string().ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
            let error = body["error"].as_str().unwrap_or("error");
            let message = body["message"].as_str().unwrap_or("");
            Err(format!("{error} ({code}): {message}"))
        }
        Err(e) => Err(format!("cannot reach the skin server: {e}")),
    }
}
