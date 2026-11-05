// Run: node --test worker/test/
// In-memory KV + fake MineSkin; exercises every route and every guard.
import { test } from "node:test";
import assert from "node:assert/strict";
import worker, { msgRegister, msgSkin, msgRecover, recoveryHash } from "../src/index.js";

class MemKV {
  constructor() { this.m = new Map(); }
  async get(k, type) { const v = this.m.get(k); if (v == null) return null; return type === "json" ? JSON.parse(v) : v; }
  async put(k, v) { this.m.set(k, v); }
  async delete(k) { this.m.delete(k); }
  async list({ prefix }) { return { keys: [...this.m.keys()].filter((k) => k.startsWith(prefix)).map((name) => ({ name })) }; }
}

let mineskinCalls = 0;
function makeEnv(extra = {}) {
  return {
    SKINS: new MemKV(), PEPPER: "pepper-test", ADMIN_TOKEN: "admin-test", MINESKIN_KEY: "k",
    MINESKIN_FETCH: async () => {
      mineskinCalls++;
      return new Response(JSON.stringify({ success: true, skin: { texture: {
        data: { value: "VALUE" + mineskinCalls, signature: "SIG" },
        url: { skin: "https://textures.minecraft.net/texture/abc" } } } }), { status: 200 });
    },
    ...extra,
  };
}

const b64 = (u8) => Buffer.from(u8).toString("base64");
async function newKey() {
  const kp = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  const pub = b64(new Uint8Array(await crypto.subtle.exportKey("raw", kp.publicKey)));
  const sign = async (m) => b64(new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, kp.privateKey, new TextEncoder().encode(m))));
  return { pub, sign };
}
function png(w = 64, h = 64) {
  const b = Buffer.alloc(40);
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(b, 0);
  b.writeUInt32BE(13, 8); b.write("IHDR", 12, "ascii"); b.writeUInt32BE(w, 16); b.writeUInt32BE(h, 20);
  return b;
}
async function call(env, method, path, body, headers = {}) {
  const init = { method, headers: { "Content-Type": "application/json", "CF-Connecting-IP": "1.2.3.4", ...headers } };
  if (body !== undefined) init.body = JSON.stringify(body);
  const r = await worker.fetch(new Request("https://x" + path, init), env);
  return { status: r.status, body: await r.json() };
}
let clock = Math.floor(Date.now() / 1000);
const ts = () => ++clock; // strictly increasing, inside the 5-minute window

async function registered(env, name = "Rilex") {
  const k = await newKey();
  const rh = await recoveryHash("RLX-AAAA-BBBB-CCCC-DDDD");
  const t = ts();
  const r = await call(env, "POST", "/v1/register", { name, pubkey: k.pub, recovery_hash: rh, ts: t, sig: await k.sign(msgRegister(name.toLowerCase(), k.pub, rh, t)) });
  assert.equal(r.status, 201, JSON.stringify(r.body));
  return k;
}
async function upload(env, k, name, bytes, model = "classic", t = ts()) {
  const sha = (await import("node:crypto")).createHash("sha256").update(bytes).digest("hex");
  return call(env, "POST", "/v1/skin", { name, model, png: b64(bytes), ts: t, sig: await k.sign(msgSkin(name.toLowerCase(), model, sha, t)) });
}

test("register, upload, read back", async () => {
  const env = makeEnv();
  const k = await registered(env);
  const up = await upload(env, k, "Rilex", png());
  assert.equal(up.status, 200, JSON.stringify(up.body));
  assert.equal(up.body.profile.texture.signature, "SIG");
  const g = await call(env, "GET", "/v1/skin/RILEX");
  assert.equal(g.status, 200);
  assert.equal(g.body.profile.rec, undefined, "recovery hash must never be public");
  assert.equal(g.body.profile.last_ts, undefined);
  const batch = await call(env, "GET", "/v1/skins?names=rilex,nobody");
  assert.deepEqual(Object.keys(batch.body.profiles), ["rilex"]);
});

test("name is first come, first served", async () => {
  const env = makeEnv();
  await registered(env, "Rilex");
  const k2 = await newKey();
  const rh = await recoveryHash("X"); const t = ts();
  const r = await call(env, "POST", "/v1/register", { name: "RILEX", pubkey: k2.pub, recovery_hash: rh, ts: t, sig: await k2.sign(msgRegister("rilex", k2.pub, rh, t)) });
  assert.equal(r.status, 409);
});

test("someone else's key cannot change my skin", async () => {
  const env = makeEnv();
  await registered(env);
  const thief = await newKey();
  const r = await upload(env, thief, "Rilex", png());
  assert.equal(r.status, 401);
});

test("replayed and stale timestamps are refused", async () => {
  const env = makeEnv();
  const k = await registered(env);
  const t = ts();
  assert.equal((await upload(env, k, "Rilex", png(), "classic", t)).status, 200);
  const replay = await upload(env, k, "Rilex", png(64, 32), "classic", t);
  assert.equal(replay.body.error, "replayed_ts");
  const stale = await upload(env, k, "Rilex", png(), "classic", Math.floor(Date.now() / 1000) - 3600);
  assert.equal(stale.body.error, "stale_ts");
});

test("bad images are refused before MineSkin", async () => {
  const env = makeEnv();
  const k = await registered(env);
  const before = mineskinCalls;
  assert.equal((await upload(env, k, "Rilex", png(128, 128))).body.error, "bad_png");
  assert.equal((await upload(env, k, "Rilex", Buffer.from("not a png"))).body.error, "bad_png");
  assert.equal((await upload(env, k, "Rilex", Buffer.alloc(30000))).status, 413);
  assert.equal(mineskinCalls, before);
});

test("upload cooldown, and the same image costs no MineSkin call", async () => {
  const env = makeEnv();
  const k = await registered(env);
  assert.equal((await upload(env, k, "Rilex", png())).status, 200);
  const calls = mineskinCalls;
  const same = await upload(env, k, "Rilex", png());
  assert.equal(same.body.unchanged, true);
  assert.equal(mineskinCalls, calls);
  const other = await upload(env, k, "Rilex", png(64, 32));
  assert.equal(other.body.error, "cooldown");
});

test("recovery: wrong code locks after 5 tries, right code waits for admin, old key dies", async () => {
  const env = makeEnv();
  const oldKey = await registered(env);
  const nk = await newKey();
  const newRh = await recoveryHash("NEW1-NEW2-NEW3-NEW4");
  const ask = async (code) => { const t = ts(); return call(env, "POST", "/v1/recover", { name: "Rilex", code, new_pubkey: nk.pub, new_recovery_hash: newRh, ts: t, sig: await nk.sign(msgRecover("rilex", nk.pub, newRh, t)) }); };

  const ok = await ask("rlx aaaa bbbb cccc dddd"); // case/spaces/dashes don't matter
  assert.equal(ok.status, 202);
  assert.equal(ok.body.status, "pending");

  const unauth = await call(env, "GET", "/v1/admin/pending");
  assert.equal(unauth.status, 401);
  const pend = await call(env, "GET", "/v1/admin/pending", undefined, { Authorization: "Bearer admin-test" });
  assert.equal(pend.body.pending.length, 1);
  const ap = await call(env, "POST", "/v1/admin/approve", { name: "rilex" }, { Authorization: "Bearer admin-test" });
  assert.equal(ap.body.status, "approved");

  assert.equal((await upload(env, oldKey, "Rilex", png())).status, 401, "old key must stop working");
  assert.equal((await upload(env, nk, "Rilex", png())).status, 200, "new key works");

  // the old code is spent now
  for (let i = 0; i < 4; i++) assert.equal((await ask("RLX-AAAA-BBBB-CCCC-DDDD")).body.error, "wrong_code");
  assert.equal((await ask("RLX-AAAA-BBBB-CCCC-DDDD")).body.error, "wrong_code");
  assert.equal((await ask("NEW1-NEW2-NEW3-NEW4")).body.error, "locked", "locked even with the right code");
});

test("auto approve", async () => {
  const env = makeEnv({ AUTO_APPROVE: "true" });
  await registered(env);
  const nk = await newKey(); const rh = await recoveryHash("Z"); const t = ts();
  const r = await call(env, "POST", "/v1/recover", { name: "Rilex", code: "RLX-AAAA-BBBB-CCCC-DDDD", new_pubkey: nk.pub, new_recovery_hash: rh, ts: t, sig: await nk.sign(msgRecover("rilex", nk.pub, rh, t)) });
  assert.equal(r.body.status, "approved");
});

test("admin can clear a skin and release a name", async () => {
  const env = makeEnv();
  const k = await registered(env);
  await upload(env, k, "Rilex", png());
  const H = { Authorization: "Bearer admin-test" };
  await call(env, "POST", "/v1/admin/clear-skin", { name: "Rilex" }, H);
  assert.equal((await call(env, "GET", "/v1/skin/rilex")).body.profile.texture, null);
  await call(env, "POST", "/v1/admin/release", { name: "Rilex" }, H);
  assert.equal((await call(env, "GET", "/v1/skin/rilex")).status, 404);
});

test("register rate limit per IP", async () => {
  const env = makeEnv();
  for (let i = 0; i < 5; i++) await registered(env, "user_" + i);
  const k = await newKey(); const rh = await recoveryHash("x"); const t = ts();
  const r = await call(env, "POST", "/v1/register", { name: "user_9", pubkey: k.pub, recovery_hash: rh, ts: t, sig: await k.sign(msgRegister("user_9", k.pub, rh, t)) });
  assert.equal(r.status, 429);
});

// Cross-language vector: RFC 8032 test key 1, signing the canonical register message.
// engine/src/keys.rs asserts ring produces this exact signature.
test("print cross-language vector", async () => {
  const seed = Buffer.from("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60", "hex");
  const pkcs8 = Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), seed]);
  const priv = await crypto.subtle.importKey("pkcs8", pkcs8, { name: "Ed25519" }, false, ["sign"]);
  const pub = "11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo="; // RFC 8032 key 1 public key
  const rh = await recoveryHash("RLX-AAAA-BBBB-CCCC-DDDD");
  const m = msgRegister("rilex", pub, rh, 1762370000);
  const sig = b64(new Uint8Array(await crypto.subtle.sign({ name: "Ed25519" }, priv, new TextEncoder().encode(m))));
  console.log("VECTOR recovery_hash", rh);
  console.log("VECTOR sig", sig);
});
