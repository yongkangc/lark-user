# Initial read foundation

International Lark is the primary target. This revision has no verified
international web protocol: no authenticated session, browser debugging endpoint,
tenant URL, or approved read destination was supplied to the development process.
Feishu reference implementations are research evidence, not international routes.

## Ownership and data

The `lark_user` library owns session import, protected local storage, normalized
models, SQLite archives, bounded context generation and passive HAR inspection.
The `lark-user` binary owns argument parsing, stdin/files, JSON output and exit
codes. External agents own planning. No MCP, embedded model, browser automation,
official OAuth, bot credentials, network telemetry or implicit sending is added.

A profile has a user-supplied HTTPS web origin and optional declared account and
tenant IDs. Identity is not inferred from cookie names. Archives require both
declared IDs, pin their scope on creation, and reject mismatching records.
Rebinding an existing archive or an imported session to another origin is denied.
Profiles occupy separate private directories. Local archive data is explicitly
distinguished from current server data.

Playwright cookie arrays and storage-state objects are accepted from bounded files
or stdin. Cookie attributes and extension fields survive encrypted persistence.
Debug and error paths never print cookie values. OS keyring storage protects each
profile's random vault key. Headless Unix machines can explicitly select a
separately stored 0600 key file; there is no automatic plaintext fallback.
Credentials are encrypted using XChaCha20-Poly1305, bound to the store and profile,
and written atomically. Private files reject symlinks and unsafe permissions.
Per-profile advisory locks serialize credential/configuration/archive mutation.

## Evidence boundary

`auth status` reports locally imported and unverified state, never an authenticated
account. All expired cookies produce `AUTH_EXPIRED`; metadata alone cannot prove
that an apparently unexpired session is usable. No session-refresh endpoint is
called without verifying it against the user's international session.

Online chat discovery, message reads, server search, threads, sync, send/reply and
watch fail with `PROTOCOL_UNVERIFIED`. A send dry run describes only the requested
intent and states that it is not sendable; it performs no network or archive
mutation. Passive HAR inspection reports origins, header names, numeric command
IDs, response statuses and MIME types without printing bodies, cookies, query
strings or unknown paths. It does not infer read safety, tenant identity or schemas.

## Independent functionality

An optional archive ingests explicitly supplied normalized JSONL, records import
coverage as incomplete, deduplicates by chat/message within the pinned scope,
preserves newer revisions over older ones, and rejects conflicting equal revisions.
SQLite FTS is local search only. Explicit local reads use stable keyset cursors
bound to scope and filters; changed filters and invalid cursors are rejected.
Deletion tombstones scrub message content. Explicit pruning and clearing remove
message/FTS content with SQLite secure deletion and vacuuming. Local retention is
configurable and cannot establish or override enterprise retention policy.

Context selects recent local messages, returns oldest-to-newest within the selected
window, and bounds both message count and serialized bytes. It includes source
identities, local coverage and omissions. All message content is labeled untrusted.
Oversized records are omitted rather than silently truncated or misattributed.

## Verification

Deterministic synthetic fixtures verify cookie attributes and redaction, encrypted
storage and tamper detection, expired sessions, scope isolation, stable pagination,
cursor binding, deduplication/revision handling, FTS deletion, bounded context,
HAR secret exclusion and real CLI exit/output behavior. Rust formatting, clippy,
tests and binary startup are required. A working live read needs an approved chat,
exact captured regional routes, authentication requirements and response decoding
validated against that same web session. Sending needs a separately approved
destination. Synthetic tests never establish live Lark support.
