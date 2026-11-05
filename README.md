# RILEX SKINS

Offline skins for Horizon (rilexDEV/horizon-custom): every player claims a name with a key,
uploads a skin, and everyone with the mod sees it. Servers can show it to vanilla players too
(phase 3), because the registry stores Mojang-signed textures from MineSkin.

```
worker/   Cloudflare Worker — the registry (names, keys, skins, recovery, admin)
mod/rilex_skins/
  compatibility.json
  mixins/rilex_skins/
    horizon-mixin.toml   the manifest
    command.rs tick.rs list.rs   glue helpers (hook points: command, tick, mods)
    engine/              the mod's crate: keys, recovery codes, HTTP, commands (cargo test here)
```

## Security, in one table

| Layer | Where |
|---|---|
| Ed25519 key per player, private key never leaves the computer | engine/src/keys.rs |
| First come, first served: a name is bound to its key | worker `register` |
| Every write signed, with a timestamp (5 min window, no replays) | worker `checkTs` |
| SHA-256 of the PNG inside the signature; 64x64, < 24 KB checked before MineSkin | worker `uploadSkin` |
| Mojang signature on the texture (MineSkin, called from the Worker: the API key stays secret) | worker `mineskinSign` |
| Recovery code: only its hash is sent, stored peppered; 5 wrong tries lock the name 24 h | worker `recover` |
| Lost key → new key + code → admin approves → old key and old code die | `/v1/admin/approve` |

## 1. Deploy the Worker (once)

```bash
cd worker
npm test                                   # 11 tests, no install needed (Node 22)
npx wrangler login
npx wrangler kv namespace create SKINS     # paste the id into wrangler.toml
npx wrangler secret put MINESKIN_KEY       # free key: https://account.mineskin.org/keys
npx wrangler secret put ADMIN_TOKEN        # any long random string; keep it
npx wrangler secret put PEPPER             # another long random string; never change it
npx wrangler deploy                        # prints https://rilex-skins.<you>.workers.dev
```

Admin (recoveries, bad skins):

```bash
T=your-admin-token; U=https://rilex-skins.<you>.workers.dev
curl -H "Authorization: Bearer $T" $U/v1/admin/pending
curl -H "Authorization: Bearer $T" -d '{"name":"Rilex"}' $U/v1/admin/approve   # or reject
curl -H "Authorization: Bearer $T" -d '{"name":"Bad"}'   $U/v1/admin/clear-skin
curl -H "Authorization: Bearer $T" -d '{"name":"Bad"}'   $U/v1/admin/release
```

## 2. Build the mod

1. `engine/src/lib.rs`: set `DEFAULT_API` to your Worker's address.
2. `horizon-mixin.toml` and `compatibility.json`: put your fork's commit
   (`git -C horizon-custom rev-parse HEAD`) in `horizon_revision`.
3. Test the crate: `cd mod/rilex_skins/mixins/rilex_skins/engine && cargo test`
   — `same_signature_as_the_worker` proves the mod and the Worker speak the same protocol.
4. Build the game with it:
   `horizon-launcher native-build mod/rilex_skins/mixins/rilex_skins --trust-native-code`

## 3. In game

```
/rilexskin register Rilex              claim the name; WRITE DOWN the recovery code
/rilexskin set C:\skins\me.png slim    upload (classic or slim)
/rilexskin backup                      key file for a USB stick / phone
/rilexskin restore <file>              new computer, same name
/rilexskin recover Rilex RLX-....      lost everything: code -> admin approves -> /rilexskin finish
```

The command is `/rilexskin`, not `/skin`: servers with SkinsRestorer use `/skin`.

## Next

- Phase 2: draw the registry skins on other players (needs the fork's skin lookup function).
- Phase 3: a server-side part that applies the signed textures, so vanilla players see them.
