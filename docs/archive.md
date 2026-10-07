# Local archive contract

`archive import` explicitly ingests normalized JSONL, not an assumed Lark wire
format. Each line is a `model::Message`; the [fixture](../tests/fixtures/normalized.jsonl)
is entirely synthetic. Required: account/tenant scope, chat/message IDs, RFC3339
sent timestamp and message type. Optional: sender, edit/update time, thread, text,
mentions, quotes, attachment metadata and unsupported structured payload.

Imports require private `0600` files or stdin and are bounded to 32 MiB, 10000
records and 1 MiB per line. Profiles pin account/tenant identity. Wrong-scope
records reject the entire import. Deduplication is by `(chat_id,message_id)` in
that scope. Identical records are duplicates; newer revisions replace older ones;
stale revisions are skipped. Equal-revision conflicts roll back the batch.
Revision time is the latest sent/edited/updated time. Deletion tombstones require
a newer revision and scrub text, mentions, quotes, attachments, payload and link.
Imports cannot observe remote permissions or deleted history.

Source links from imports remain user-supplied/unverified. Credential authority,
queries and fragments are rejected until genuine deep-link formats are verified.
Sources still have explicit account/tenant/chat/message/thread identities. Unknown
content stays structured rather than being fabricated as text.

Each profile uses a private SQLite database and advisory locks. SQLite plus FTS5
secure deletion remove old text/token history on edits/deletions. Explicit
pruning/clearing additionally rebuilds FTS and vacuums. See
[SQLite's FTS5 secure-delete contract](https://www.sqlite.org/fts5.html#the_secure_delete_configuration_option).
The archive is **not encrypted at rest**; use host encryption and approved
enterprise storage policy. Credentials are in a separate encrypted vault.
Deletion does not cover backups, exported files, snapshots or device remanence.

Retention defaults to 30 days, configurable to 1–3650. Imports exclude/prune older
rows. Changing retention requires `archive prune` to remove previously stored
rows. Reads do not silently delete data. This setting cannot override enterprise
retention restrictions, whose exact semantics are not verified.

Messages sort by `(sent time,message ID,chat ID)` descending. Keyset cursors bind
scope, filters and limit; local edits/imports between pages are not a snapshot.
Use bounded overlap/deduplication if reading a changing archive. Cursors are not
credentials. All SQL values are parameters. FTS input is a literal phrase.

Pages return source, limit, has-more/cursor and completeness. Observed first/last
timestamps do not imply contiguous coverage. Local history remains incomplete.
Context takes a bounded recent page, emits selected messages oldest first and
fits `--max-bytes` (2048–1048576; excluding newline). Oversized records are omitted
and counted. Sources/content are untrusted; the CLI extracts no tasks or plans.
