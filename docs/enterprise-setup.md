# Enterprise international Lark session setup

Use your account's normal enterprise entry point and SSO/QR/2FA flow. This project
cannot bypass permission, SSO or retention restrictions. It uses no developer app,
app secret, bot registration or official OAuth integration.

## Export and import

1. Complete normal sign-in and record the actual signed-in web URL.
2. Export relevant cookies including HttpOnly cookies with a trusted browser
   exporter or `context.cookies()` from an existing authorized Playwright context.
3. Save outside the repository with `0700` directory / `0600` file permissions.
4. Use `auth import --cookie-file FILE --web-url ACTUAL_URL` or stdin via file `-`.

The first import requires the actual HTTPS browser URL without credentials, query
or fragment. No international host is guessed. Supported export shape:

```json
[{"name":"example_session_cookie","value":"SYNTHETIC_VALUE",
  "domain":".tenant.example.test","path":"/","expires":4102444800,
  "httpOnly":true,"secure":true,"sameSite":"Lax"}]
```

`{"cookies":[...],"origins":[...]}` is also accepted. Domain, path, expiry,
HttpOnly, Secure, SameSite and extension attributes survive encrypted storage.
Exports require these browser cookie fields. Inputs are bounded to 4 MiB and 4096
cookies; duplicate identities and malformed records produce redacted errors.
`document.cookie` omits HttpOnly cookies and is insufficient. Storage-state
origins/localStorage are not imported: their auth role has not been established.
Partitioned cookies are preserved and require a verified context before use.

Given an already authorized Playwright context, write without printing values:

```js
import { writeFileSync, chmodSync } from "node:fs";
const cookiePath = "/private/path/cookies.json";
writeFileSync(cookiePath, JSON.stringify(await context.cookies()), { mode: 0o600 });
chmodSync(cookiePath, 0o600);
```

This is a recipe for your existing context, not browser sign-in automation inside
the CLI. Never paste cookie/token/key values into an agent prompt or upload them.

## Local credential storage

Default location: platform local application-data directory (Linux usually
`~/.local/share/lark-user`). `--store-dir` / `LARK_USER_STORE_DIR` override its path.
Profiles occupy separate `profiles/NAME` directories. Vaults use authenticated
XChaCha20-Poly1305 encryption, bound to the store/profile. Random keys live in
macOS Keychain or Linux Secret Service. Private files reject symlink leaves,
unexpected owners and group/other permissions. Writes are atomic and serialized
with per-profile locks. Windows ACL storage is currently unsupported.

An unavailable keyring returns `CREDENTIAL_STORE_UNAVAILABLE`. On headless Unix,
explicitly generate a separate `0600` 32-byte key via `auth keygen --out PATH` and
use `--key-file PATH` or path-valued `LARK_USER_KEY_FILE` for auth commands. Parent
directories must be private. Keygen never replaces a file. Keep this key outside
vault backups; access to both permits decryption. There is no automatic plaintext
fallback. Raw cookie values are never arguments, and error paths redact details.

`auth logout` removes the local vault/keyring key. It leaves a shared explicit key
file, browser session, profile and archive. It does not revoke the remote session.
Use `archive clear --yes` for archive deletion. Original exports/backups need their
own deletion policy.

## Status and renewal

Status is local: `imported_unverified`, `authenticated:null`, `live_verified:false`.
Every expired cookie yields `AUTH_EXPIRED`, exit 3. Unexpired metadata cannot
detect server revocation, SSO challenges or an expired auth cookie among live
preferences. Sign in again normally and import a fresh export. No unverified
heartbeat/refresh endpoint or official API fallback is called. Legitimate server
Set-Cookie updates will be persisted only after their international behavior and
scope are verified.

## Remaining live evidence

Capture a small private HAR from the authenticated international web client while
discovering chats, reading one approved chat, paging older messages, searching a
known message and opening a thread. Retain necessary sensitive headers/content
only in a `0600` local file. Do not send messages during this read capture. Opening
a browser chat may update read receipts; direct read requests must be reviewed
for those effects separately.

`protocol inspect --har FILE --json` summarizes origins, field names, numeric
commands, response status/MIME and numeric Retry-After. It prints no credential
values, query strings, unknown paths or bodies, and never replays traffic.
Captured browser success does not prove direct cookie-only access or read safety.

Feishu sources suggest checking CSRF/device/session headers, web version, request
IDs and WebSocket tickets. These are **source-only leads**, not verified
international requirements. Exact additional values and capture/renewal rules
must come from your regional traffic. HARs may omit binary protobuf bodies or
WebSocket frames; preserve private binary/DevTools captures where needed.

The verifier needs local paths, actual browser URL and an approved read chat ID.
Keep credentials and enterprise content out of tool output; retain only sanitized
evidence. Sending requires a separately approved destination and draft.
