# Capabilities and blockers

No authorized Lark cookie export, tenant/region URL, chat, HAR or signed-in browser
process was available during development. No international live operation has
been tested. Source research and synthetic tests establish different claims.

| Capability | Status | Evidence / remaining blocker |
| --- | --- | --- |
| Cookie import, encrypted vault, local status/logout | Tested locally | Playwright metadata, private files, redaction, expiry and tamper tests |
| OS keyring storage | Implemented; integration unverified here | Native macOS/Linux backend; tests use an explicit private key; unavailable keyring fails |
| Profile/account/tenant isolation | Tested locally | Separate directories, pinned SQLite scope, mismatching records/rebinding rejected |
| Local reads, chat catalog, thread filter, FTS search, JSONL export | Tested locally | Explicit normalized imports; incomplete local coverage |
| Bounded context and agent planning example | Tested locally | JSON byte/message limits, sources, chronological window and untrusted labels |
| HAR inspection | Experimental research helper | Passive origin/header/status/command extraction only |
| Live authentication and cookie renewal | Blocked/unverified | Actual regional auth route, identity schema, CSRF/Set-Cookie behavior needed |
| Server chat discovery and enterprise DMs/groups | Blocked/unverified | Actual cookie-only catalog route, cursors and permission semantics needed |
| Server message reads/get/export | Blocked/unverified | Approved chat, request/response envelope, timestamp units, paging and retention/deletions needed |
| Server search | Blocked/unverified | International cookie-only query/filter/result/cursor schemas needed |
| Server threads | Blocked/unverified | Thread route, relation and pagination semantics needed; root ID equivalence not assumed |
| Incremental server sync | Blocked/unverified | Bounded history paging plus edits/deletions/gap recovery needed |
| Send/reply | Blocked/unverified | International wire format, delivery proof, approved destination and draft needed |
| Send/reply dry run | Tested intent preview | `sendable:false`, bytes/hash only, zero requests |
| WebSocket / polling watch | Blocked/unverified | Regional ticket/frontier/frame/ACK/recovery or verified bounded polling route/cadence needed |
| Feishu transport | Optional; not implemented | Source research exists; must not delay international Lark |

Recognized online commands return `PROTOCOL_UNVERIFIED`, exit 5. They never return
empty success, guess hosts or switch to OAuth/bot identity. `--local` explicitly
selects imported records. `has_more` is local; coverage always records unknown
contiguous history, unverified input and unobserved enterprise deletions/retention.
Rate limits remain unknown; a HAR Retry-After observation is not a quota.

Live acceptance requires a confirmed read-safe request with imported cookies to
the approved actual regional origin, verified account/tenant identity, results
compared with the authorized web chat, and paging/auth-expiry/permission tests.
Unknown protobuf content must remain opaque structured data. Permission/SSO/
retention failures must be surfaced. Fake-server tests cannot satisfy live proof.
