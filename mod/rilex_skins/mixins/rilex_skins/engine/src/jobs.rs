// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// Network work runs on its own thread; its result lines wait here until the game's tick
// prints them in chat (tick.rs). One job at a time, so a double command can't race.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static OUT: Mutex<Vec<String>> = Mutex::new(Vec::new());
static BUSY: AtomicBool = AtomicBool::new(false);

pub fn say(line: impl Into<String>) {
    if let Ok(mut out) = OUT.lock() {
        out.push(line.into());
    }
}

/// Lines to show now (called every tick; cheap when empty).
pub fn drain() -> Vec<String> {
    match OUT.lock() {
        Ok(mut out) if !out.is_empty() => std::mem::take(&mut *out),
        _ => Vec::new(),
    }
}

/// Runs `job` on a background thread unless one is already running.
pub fn spawn(job: impl FnOnce() -> Vec<String> + Send + 'static) -> bool {
    if BUSY.swap(true, Ordering::AcqRel) {
        return false;
    }
    std::thread::spawn(move || {
        let lines = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
            .unwrap_or_else(|_| vec!["[RILEX SKINS] something went wrong; try again.".to_string()]);
        for line in lines {
            say(line);
        }
        BUSY.store(false, Ordering::Release);
    });
    true
}
