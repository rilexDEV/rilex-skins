// RILEX SKINS — © 2026 RILEX MODS (rilexDEV). MIT licence: keep this notice in every copy.
// RILEX SKINS registry — Cloudflare Worker.
//
// Who may write what:
//   * a name is claimed once with an Ed25519 public key (first come, first served);
//   * every later write for that name must be signed by that key;
//   * a lost key is replaced only with the recovery code (hashed, peppered) + admin approval;
//   * MineSkin is called from here, so the API key never ships inside the mod.
//
// Bindings (wrangler.toml / dashboard):
//   SKINS          KV namespace
//   MINESKIN_KEY   secret  — MineSkin API key
//   ADMIN_TOKEN    secret  — long random string, for /v1/admin/*
//   PEPPER         secret  — long random string, mixed into stored recovery hashes
//   AUTO_APPROVE   var     — "true" to approve recoveries without the admin (default: off)

const NAME_RE = /^[A-Za-z0-9_]{3,16}$/;
const HEX64_RE = /^[0-9a-f]{64}$/;
const MAX_SKEW_S = 300;          // a signed request is valid for 5 minutes
const MAX_PNG_BYTES = 24 * 1024; // a 64x64 skin is a few KB
const UPLOAD_COOLDOWN_S = 60;    // one skin upload per name per minute
const REGISTER_PER_IP_HOUR = 5;
const RECOVER_MAX_FAILS = 5;
const RECOVER_LOCK_S = 24 * 3600;
const BATCH_MAX = 50;

const enc = new TextEncoder();

// ---------- canonical messages (must match engine/src/proto.rs byte for byte) ----------

export function msgRegister(lname, pubkey, recoveryHash, ts) {
  return ["rilex-skins/v1/register", lname, pubkey, recoveryHash, String(ts)].join("\n");
}
export function msgSkin(lname, model, skinSha256, ts) {
  return ["rilex-skins/v1/skin", lname, model, skinSha256, String(ts)].join("\n");
}
export function msgRecover(lname, newPubkey, newRecoveryHash, ts) {
  return ["rilex-skins/v1/recover", lname, newPubkey, newRecoveryHash, String(ts)].join("\n");
}
export function normalizeCode(code) {
  return String(code).toUpperCase().replace(/[^A-Z0-9]/g, "");
}
export async function recoveryHash(code) {
  return sha256Hex(enc.encode("rilex-skins/v1/recovery\n" + normalizeCode(code)));
}

// ---------- small helpers ----------

function b64ToBytes(s) {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
async function sha256Hex(bytes) {
  const d = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...d].map((b) => b.toString(16).padStart(2, "0")).join("");
}
function timingSafeEqual(a, b) {
  const x = enc.encode(String(a)), y = enc.encode(String(b));
  let diff = x.length ^ y.length;
  for (let i = 0; i < Math.max(x.length, y.length); i++) diff |= (x[i] ?? 0) ^ (y[i] ?? 0);
  return diff === 0;
}
async function verifyEd25519(pubkeyB64, sigB64, message) {
  try {
    const raw = b64ToBytes(pubkeyB64);
    const sig = b64ToBytes(sigB64);
    if (raw.length !== 32 || sig.length !== 64) return false;
    const key = await crypto.subtle.importKey("raw", raw, { name: "Ed25519" }, false, ["verify"]);
    return await crypto.subtle.verify({ name: "Ed25519" }, key, sig, enc.encode(message));
  } catch {
    return false;
  }
}
function validPubkey(s) {
  try { return typeof s === "string" && b64ToBytes(s).length === 32; } catch { return false; }
}
export function pngDims(bytes) {
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  if (bytes.length < 24 || sig.some((v, i) => bytes[i] !== v)) return null;
  if (String.fromCharCode(...bytes.slice(12, 16)) !== "IHDR") return null;
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { w: dv.getUint32(16), h: dv.getUint32(20) };
}
const now = () => Math.floor(Date.now() / 1000);

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};
function json(body, status = 200, extra = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8", ...CORS, ...extra },
  });
}
const err = (status, code, message) => json({ ok: false, error: code, message }, status);

async function readJson(req) {
  if ((req.headers.get("Content-Length") | 0) > 64 * 1024) return null;
  try { return await req.json(); } catch { return null; }
}
function checkTs(ts, lastTs = 0) {
  if (!Number.isInteger(ts)) return "bad_ts";
  if (Math.abs(now() - ts) > MAX_SKEW_S) return "stale_ts";   // check your computer's clock
  if (ts <= lastTs) return "replayed_ts";
  return null;
}

// Public view of a profile: never the recovery hash or bookkeeping.
function publicView(p) {
  return {
    name: p.name,
    pubkey: p.pubkey,
    model: p.model ?? null,
    skin_sha256: p.skin_sha256 ?? null,
    texture: p.texture ?? null,
    updated: p.updated ?? null,
  };
}

// ---------- MineSkin ----------

async function mineskinSign(env, png, model, lname) {
  const fd = new FormData();
  fd.append("file", new Blob([png], { type: "image/png" }), "skin.png");
  fd.append("variant", model === "slim" ? "slim" : "classic");
  fd.append("visibility", "unlisted");
  fd.append("name", lname);
  const doFetch = env.MINESKIN_FETCH ?? fetch; // tests inject a fake
  const r = await doFetch("https://api.mineskin.org/v2/generate", {
    method: "POST",
    headers: { Authorization: `Bearer ${env.MINESKIN_KEY}`, "User-Agent": "RILEX-Skins-Registry/0.1" },
    body: fd,
  });
  let j = null;
  try { j = await r.json(); } catch { /* fall through */ }
  const data = j?.skin?.texture?.data;
  if (!r.ok || !data?.value || !data?.signature) {
    const why = j?.errors?.[0]?.message || j?.message || `HTTP ${r.status}`;
    return { error: why };
  }
  return { value: data.value, signature: data.signature, url: j?.skin?.texture?.url?.skin ?? null };
}

// ---------- handlers ----------

async function getSkin(env, name) {
  if (!NAME_RE.test(name)) return err(400, "bad_name", "3-16 letters, digits or _");
  const p = await env.SKINS.get("p:" + name.toLowerCase(), "json");
  if (!p) return err(404, "not_found", "no skin for this name");
  return json({ ok: true, profile: publicView(p) }, 200, { "Cache-Control": "public, max-age=60" });
}

async function getBatch(env, url) {
  const names = (url.searchParams.get("names") || "").split(",").map((s) => s.trim()).filter(Boolean);
  if (names.length === 0 || names.length > BATCH_MAX) return err(400, "bad_names", `1-${BATCH_MAX} names`);
  const out = {};
  for (const n of names) {
    if (!NAME_RE.test(n)) continue;
    const p = await env.SKINS.get("p:" + n.toLowerCase(), "json");
    if (p?.texture) out[n.toLowerCase()] = publicView(p);
  }
  return json({ ok: true, profiles: out }, 200, { "Cache-Control": "public, max-age=60" });
}

async function register(env, req) {
  const b = await readJson(req);
  if (!b) return err(400, "bad_json", "body must be JSON");
  const { name, pubkey, recovery_hash, ts, sig } = b;
  if (!NAME_RE.test(name ?? "")) return err(400, "bad_name", "3-16 letters, digits or _");
  if (!validPubkey(pubkey)) return err(400, "bad_pubkey", "32-byte Ed25519 key, base64");
  if (!HEX64_RE.test(recovery_hash ?? "")) return err(400, "bad_recovery_hash", "64 hex chars");
  const tsErr = checkTs(ts);
  if (tsErr) return err(400, tsErr, "timestamp rejected");
  const lname = name.toLowerCase();
  if (!(await verifyEd25519(pubkey, sig, msgRegister(lname, pubkey, recovery_hash, ts))))
    return err(401, "bad_sig", "signature does not match");

  const ip = req.headers.get("CF-Connecting-IP") || "unknown";
  const ipKey = "ip:reg:" + ip;
  const count = Number((await env.SKINS.get(ipKey)) || 0);
  if (count >= REGISTER_PER_IP_HOUR) return err(429, "rate_limited", "too many registrations, try in an hour");

  const key = "p:" + lname;
  if (await env.SKINS.get(key)) return err(409, "name_taken", "this name is already registered");
  const profile = {
    name, pubkey,
    rec: await sha256Hex(enc.encode(env.PEPPER + ":" + recovery_hash)),
    created: now(), last_ts: ts, updated: now(),
  };
  await env.SKINS.put(key, JSON.stringify(profile));
  await env.SKINS.put(ipKey, String(count + 1), { expirationTtl: 3600 });
  return json({ ok: true, profile: publicView(profile) }, 201);
}

async function uploadSkin(env, req) {
  const b = await readJson(req);
  if (!b) return err(400, "bad_json", "body must be JSON");
  const { name, model, png, ts, sig } = b;
  if (!NAME_RE.test(name ?? "")) return err(400, "bad_name", "3-16 letters, digits or _");
  if (model !== "classic" && model !== "slim") return err(400, "bad_model", "classic or slim");
  const lname = name.toLowerCase();
  const key = "p:" + lname;
  const p = await env.SKINS.get(key, "json");
  if (!p) return err(404, "not_registered", "register the name first");

  let bytes;
  try { bytes = b64ToBytes(png ?? ""); } catch { return err(400, "bad_png", "png must be base64"); }
  if (bytes.length === 0 || bytes.length > MAX_PNG_BYTES) return err(413, "png_size", `PNG must be under ${MAX_PNG_BYTES} bytes`);
  const dims = pngDims(bytes);
  if (!dims || dims.w !== 64 || (dims.h !== 64 && dims.h !== 32))
    return err(400, "bad_png", "skin must be a 64x64 (or 64x32) PNG");
  const skinSha = await sha256Hex(bytes);

  const tsErr = checkTs(ts, p.last_ts || 0);
  if (tsErr) return err(400, tsErr, "timestamp rejected");
  if (!(await verifyEd25519(p.pubkey, sig, msgSkin(lname, model, skinSha, ts))))
    return err(401, "bad_sig", "signature does not match this name's key");
  // Same image again: keep the signed texture, no MineSkin call (and no cooldown).
  if (p.skin_sha256 === skinSha && p.model === model && p.texture) {
    p.last_ts = ts;
    await env.SKINS.put(key, JSON.stringify(p));
    return json({ ok: true, profile: publicView(p), unchanged: true });
  }
  if (p.last_upload && now() - p.last_upload < UPLOAD_COOLDOWN_S)
    return err(429, "cooldown", `wait ${UPLOAD_COOLDOWN_S}s between uploads`);

  p.last_ts = ts; // spend the timestamp before the slow call
  p.last_upload = now();
  await env.SKINS.put(key, JSON.stringify(p));
  const signed = await mineskinSign(env, bytes, model, lname);
  if (signed.error) return err(502, "mineskin", "MineSkin: " + signed.error);

  Object.assign(p, { model, skin_sha256: skinSha, texture: signed, updated: now() });
  await env.SKINS.put(key, JSON.stringify(p));
  return json({ ok: true, profile: publicView(p) });
}

async function recover(env, req) {
  const b = await readJson(req);
  if (!b) return err(400, "bad_json", "body must be JSON");
  const { name, code, new_pubkey, new_recovery_hash, ts, sig } = b;
  if (!NAME_RE.test(name ?? "")) return err(400, "bad_name", "3-16 letters, digits or _");
  if (!validPubkey(new_pubkey)) return err(400, "bad_pubkey", "32-byte Ed25519 key, base64");
  if (!HEX64_RE.test(new_recovery_hash ?? "")) return err(400, "bad_recovery_hash", "64 hex chars");
  const lname = name.toLowerCase();
  const p = await env.SKINS.get("p:" + lname, "json");
  if (!p) return err(404, "not_registered", "no such name");

  const failKey = "f:" + lname;
  const fails = (await env.SKINS.get(failKey, "json")) || { count: 0, until: 0 };
  if (fails.until > now()) return err(423, "locked", "too many wrong codes; try again tomorrow");

  const tsErr = checkTs(ts);
  if (tsErr) return err(400, tsErr, "timestamp rejected");
  // Proof that the requester holds the NEW key.
  if (!(await verifyEd25519(new_pubkey, sig, msgRecover(lname, new_pubkey, new_recovery_hash, ts))))
    return err(401, "bad_sig", "signature does not match the new key");

  const given = await sha256Hex(enc.encode(env.PEPPER + ":" + (await recoveryHash(code ?? ""))));
  if (!timingSafeEqual(given, p.rec)) {
    fails.count += 1;
    if (fails.count >= RECOVER_MAX_FAILS) { fails.until = now() + RECOVER_LOCK_S; fails.count = 0; }
    await env.SKINS.put(failKey, JSON.stringify(fails), { expirationTtl: RECOVER_LOCK_S * 2 });
    return err(403, "wrong_code", "recovery code is not right");
  }
  await env.SKINS.delete(failKey);

  const pending = { name: p.name, new_pubkey, new_rec: new_recovery_hash, ts, asked: now() };
  if (env.AUTO_APPROVE === "true") {
    await applyRecovery(env, lname, pending);
    return json({ ok: true, status: "approved" });
  }
  await env.SKINS.put("r:" + lname, JSON.stringify(pending), { expirationTtl: 14 * 24 * 3600 });
  return json({ ok: true, status: "pending", message: "code accepted; waiting for the admin" }, 202);
}

async function applyRecovery(env, lname, pending) {
  const p = await env.SKINS.get("p:" + lname, "json");
  if (!p) return false;
  p.pubkey = pending.new_pubkey;
  p.rec = await sha256Hex(enc.encode(env.PEPPER + ":" + pending.new_rec)); // old code stops working
  p.last_ts = pending.ts;
  p.updated = now();
  await env.SKINS.put("p:" + lname, JSON.stringify(p));
  await env.SKINS.delete("r:" + lname);
  return true;
}

async function admin(env, req, action) {
  const auth = req.headers.get("Authorization") || "";
  if (!env.ADMIN_TOKEN || !timingSafeEqual(auth, "Bearer " + env.ADMIN_TOKEN))
    return err(401, "unauthorized", "admin token required");
  if (action === "pending" && req.method === "GET") {
    const list = await env.SKINS.list({ prefix: "r:" });
    const items = [];
    for (const k of list.keys) {
      const r = await env.SKINS.get(k.name, "json");
      if (r) items.push({ name: r.name, asked: r.asked });
    }
    return json({ ok: true, pending: items });
  }
  if (req.method !== "POST") return err(405, "method", "POST");
  const b = await readJson(req);
  const lname = String(b?.name ?? "").toLowerCase();
  if (!NAME_RE.test(lname)) return err(400, "bad_name", "name required");
  if (action === "approve") {
    const r = await env.SKINS.get("r:" + lname, "json");
    if (!r) return err(404, "no_pending", "no pending recovery for this name");
    await applyRecovery(env, lname, r);
    return json({ ok: true, status: "approved" });
  }
  if (action === "reject") {
    await env.SKINS.delete("r:" + lname);
    return json({ ok: true, status: "rejected" });
  }
  if (action === "clear-skin") { // offensive skin: keep the name, drop the texture
    const p = await env.SKINS.get("p:" + lname, "json");
    if (!p) return err(404, "not_found", "no such name");
    delete p.texture; delete p.skin_sha256; delete p.model; p.updated = now();
    await env.SKINS.put("p:" + lname, JSON.stringify(p));
    return json({ ok: true, status: "skin_cleared" });
  }
  if (action === "release") { // free the name entirely
    await env.SKINS.delete("p:" + lname);
    await env.SKINS.delete("r:" + lname);
    await env.SKINS.delete("f:" + lname);
    return json({ ok: true, status: "released" });
  }
  return err(404, "unknown_action", action);
}

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
    const path = url.pathname.replace(/\/+$/, "");
    try {
      if (req.method === "GET" && path.startsWith("/v1/skin/")) return await getSkin(env, decodeURIComponent(path.slice(9)));
      if (req.method === "GET" && path === "/v1/skins") return await getBatch(env, url);
      if (req.method === "POST" && path === "/v1/register") return await register(env, req);
      if (req.method === "POST" && path === "/v1/skin") return await uploadSkin(env, req);
      if (req.method === "POST" && path === "/v1/recover") return await recover(env, req);
      if (path.startsWith("/v1/admin/")) return await admin(env, req, path.slice(10));
      if (path === "" || path === "/v1") return json({ ok: true, service: "rilex-skins", version: 1 });
      return err(404, "not_found", "unknown route");
    } catch (e) {
      return err(500, "internal", "unexpected error");
    }
  },
};
