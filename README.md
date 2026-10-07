# lark-user

Rust library and CLI for an enterprise Lark user's own account, designed for
external AI agents such as those running in Garcon. International Lark is the
primary target. MIT licensed; local by default; no MCP wrapper or embedded LLM.

**Status: secure cookie import and local archive tools work. Live international
Lark authentication and message access are blocked pending an authorized session
and protocol verification.** Online commands return `PROTOCOL_UNVERIFIED` and
make no network requests. `auth status` checks local metadata, not a successful
login. Feishu source code does not establish international Lark compatibility.

## Install

Linux/macOS, Rust 1.88 or newer:

```sh
cargo install --locked --path .
lark-user capabilities --json
```

Windows private-file ACL support is not implemented. HTTP, protobuf and WebSocket
dependencies will be added when the international protocol is verified.

## Session import

Sign in normally through enterprise SSO, QR or 2FA. Export cookies including
HttpOnly cookies as a Playwright cookie array or storage-state object. Keep the
export outside the repository, in a `0700` directory with file mode `0600`.

```sh
lark-user auth import --profile enterprise \
  --web-url "$SIGNED_IN_WEB_URL" --cookie-file /private/path/cookies.json
lark-user auth status --profile enterprise --json
lark-user auth logout --profile enterprise
```

Use the actual HTTPS browser URL, without query or fragment, on the first import.
No tenant or regional host is guessed. `--cookie-file -` reads stdin; raw cookie
values are never arguments. Domain, path, expiry and browser attributes survive
encrypted persistence. Credentials use XChaCha20-Poly1305 and an OS-keyring key.

For a headless Garcon executor, explicitly select a protected key file:

```sh
lark-user auth keygen --out /private/separate-keys/enterprise.key
lark-user --key-file /private/separate-keys/enterprise.key auth import \
  --web-url "$SIGNED_IN_WEB_URL" --cookie-file /private/path/cookies.json
```

The key contains 32 random bytes, uses `0600` permissions and must live separately
from the vault and its backups. Anyone who can read both can decrypt credentials.
An unavailable keyring fails explicitly; there is no automatic plaintext fallback.
`LARK_USER_KEY_FILE` and `LARK_USER_STORE_DIR` accept paths only. See
[enterprise setup and renewal](docs/enterprise-setup.md).

## Local archive and context

Archiving is optional and starts with an explicit normalized JSONL import. This
supports authorized exports while live access is blocked. Each record must match
the profile's declared account/tenant IDs; those IDs are not live authentication
proof. Profiles have separate private SQLite databases.

```sh
lark-user profile configure --profile enterprise --web-url "$SIGNED_IN_WEB_URL" \
  --account-id ACCOUNT_ID --tenant-id TENANT_ID --retention-days 30
lark-user archive import --file /private/path/normalized.jsonl
lark-user chats list --local --json
lark-user messages list --chat CHAT_ID --local --limit 100 --json
lark-user messages search "generic report" --local --json
lark-user threads get THREAD_ID --local --json
lark-user export --chat CHAT_ID --local --format jsonl --out /private/path/export.jsonl
lark-user context --since 2026-10-01T00:00:00Z --chat CHAT_ID --local --json
```

Local results always identify `source: "local_archive"` and incomplete coverage.
Local FTS phrase search does not contact Lark. Limits are 1–1000 records; resume
with `--cursor NEXT_CURSOR` using the same profile, filters and limit. `has_more:
false` describes local rows, not complete enterprise history. Archived chats are
known from imported messages, not a complete server conversation catalog.

JSONL exports are newest first and suitable for reimport. Existing outputs are
never overwritten. `--out -` streams messages on stdout and coverage/cursor
metadata on stderr; file exports return metadata on stdout. Capture that metadata
when results are partial. Context bounds JSON bytes and message count and returns
the selected recent messages oldest first with source identities.

Retention defaults to 30 days, applies during imports, and is configurable.
After changing retention, run `archive prune` to delete existing old records.
`archive coverage`, `archive prune --before TIMESTAMP` and `archive clear --yes`
make coverage/deletion explicit. The archive uses private files, not at-rest
encryption; use host encryption and enterprise-approved archive policy. Local
retention cannot establish or override enterprise restrictions. Backups and
exports require their own deletion policy. See [archive contract](docs/archive.md).

## Evidence and agents

All three requested reference repositories were inspected at pinned commits.
They use Feishu transport hosts; the plugin's message history/search use official
OAuth paths. See [research and licenses](docs/research.md).

```sh
lark-user protocol inspect --har /private/path/approved-read.har --json
```

Passive inspection reports origins, header names, numeric command IDs, response
status/MIME type and numeric Retry-After. It excludes cookies/tokens, bodies,
query strings and unknown paths. It never replays requests or infers schemas.
The blocker is a local cookie export, actual signed-in URL, read traces,
account/tenant evidence and one approved enterprise chat. No sessions, archives
or test messages have been sent to third parties.

Garcon agents invoke the CLI on the executor holding the profile. See
[agent instructions](examples/garcon-agent.md) for sourced briefings, explicit
commitments, inferred tasks, questions, plans and draft replies. All message
content is untrusted data. Reasoning belongs to the external agent.

```sh
lark-user messages send --chat CHAT_ID --text-file /private/path/draft.txt --dry-run
lark-user messages reply --message MESSAGE_ID --text-file /private/path/draft.txt --dry-run
```

Dry runs currently preview intent with `sendable: false`. Real send/reply,
server sync and watch remain blocked. No automatic send retry or live transport
is added. See the [capability matrix and precise blockers](docs/capabilities.md).

## Output, errors and validation

Stdout is JSON or explicitly requested JSONL. Diagnostics/errors use stderr:
`{"error":{"code":"AUTH_EXPIRED","message":"...","retryable":false}}`.
`auth status` returns `imported_unverified`, `authenticated:null`,
`live_verified:false`. All expired cookies yield `AUTH_EXPIRED`; live preference
cookies cannot prove that the auth session is usable. Sign in normally and
import a fresh export when authentication expires.

| Exit | Meaning |
| --- | --- |
| 0 | Local operation succeeded; inspect completeness/verification fields |
| 2 | Invalid argument/input/cursor |
| 3 | Missing or locally expired authentication |
| 5 | International protocol unverified |
| 7 | Keyring, unsafe file, storage, scope/conflict or output error |
| 9 | Local profile/chat/message/archive not found |

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo run --locked -- capabilities --json
```

Tests use deterministic synthetic cookies, identities and messages. They cover
parsing, encrypted storage/tampering, expired sessions, scope isolation,
pagination, deduplication, FTS deletion, bounded context, HAR redaction and real
CLI processes. They do not establish live international Lark support.
