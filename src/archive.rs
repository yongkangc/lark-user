use crate::{
    Error, ErrorCode, Result,
    model::{Chat, Completeness, Message, Page, Scope, validate_id},
    private_files,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, path::Path};

pub const MAX_IMPORT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_IMPORT_MESSAGES: usize = 10000;

pub struct Archive {
    connection: Connection,
    scope: Scope,
    _lock: File,
}

enum OpenMode {
    Existing,
    Initialize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MessageQuery {
    pub chat: Option<String>,
    pub thread: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub search: Option<String>,
    pub limit: usize,
    #[serde(skip)]
    pub cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Cursor {
    binding: String,
    sent_ms: i64,
    id: String,
    chat: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub inserted: usize,
    pub updated: usize,
    pub duplicates: usize,
    pub stale: usize,
    pub outside_retention: usize,
    pub completeness: Option<Completeness>,
}

#[derive(Debug, Serialize)]
pub struct Coverage {
    pub chat_id: String,
    pub observed_first_message_at: Option<DateTime<Utc>>,
    pub observed_last_message_at: Option<DateTime<Utc>>,
    pub retained_message_count: usize,
    pub contiguous_history_verified: bool,
    pub gaps: Vec<String>,
}

impl Archive {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub fn open_existing(path: &Path, scope: Scope) -> Result<Self> {
        Self::initialize(path, scope, OpenMode::Existing)
    }

    pub fn open_or_create(path: &Path, scope: Scope) -> Result<Self> {
        Self::initialize(path, scope, OpenMode::Initialize)
    }

    fn initialize(path: &Path, scope: Scope, mode: OpenMode) -> Result<Self> {
        let create = matches!(mode, OpenMode::Initialize);
        scope.validate()?;
        if !create && !path.exists() {
            return Err(Error::new(
                ErrorCode::NotFound,
                "No local archive exists; import normalized JSONL explicitly. Online sync is not verified",
            ));
        }
        private_files::directory(path.parent().ok_or_else(Error::storage)?)?;
        let lock = private_files::open(&path.with_extension("lock"), true)?;
        lock.lock_exclusive().map_err(|_| Error::storage())?;
        private_files::open(path, create)?;
        let connection = Connection::open(path).map_err(|_| Error::storage())?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| Error::storage())?;
        connection.execute_batch("PRAGMA secure_delete=ON; PRAGMA journal_mode=DELETE; PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS scope (singleton INTEGER PRIMARY KEY CHECK(singleton=1), account_id TEXT NOT NULL, tenant_id TEXT NOT NULL);").map_err(|_| Error::storage())?;
        let existing: Option<Scope> = connection
            .query_row(
                "SELECT account_id,tenant_id FROM scope WHERE singleton=1",
                [],
                |row| {
                    Ok(Scope {
                        account_id: row.get(0)?,
                        tenant_id: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|_| Error::storage())?;
        match existing {
            Some(stored) if stored != scope => {
                return Err(Error::new(
                    ErrorCode::ScopeConflict,
                    "Archive belongs to another account or tenant",
                ));
            }
            None => {
                connection
                    .execute(
                        "INSERT INTO scope VALUES(1,?1,?2)",
                        params![scope.account_id, scope.tenant_id],
                    )
                    .map_err(|_| Error::storage())?;
            }
            _ => (),
        }
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|_| Error::storage())?;
        if version != 0 && version != 1 {
            return Err(Error::new(
                ErrorCode::StorageError,
                "Unsupported local archive schema version",
            ));
        }
        connection.execute_batch("CREATE TABLE IF NOT EXISTS messages (
            chat_id TEXT NOT NULL, message_id TEXT NOT NULL, sent_ms INTEGER NOT NULL,
            revision_ms INTEGER NOT NULL, thread_id TEXT, text TEXT NOT NULL, json TEXT NOT NULL,
            UNIQUE(chat_id,message_id));
            CREATE INDEX IF NOT EXISTS message_order ON messages(sent_ms DESC,message_id DESC,chat_id DESC);
            CREATE INDEX IF NOT EXISTS message_chat_order ON messages(chat_id,sent_ms DESC,message_id DESC);
            CREATE INDEX IF NOT EXISTS message_threads ON messages(thread_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(text,content='messages',content_rowid='rowid',tokenize='unicode61');
            INSERT INTO messages_fts(messages_fts,rank) VALUES('secure-delete',1);
            CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
                INSERT INTO messages_fts(rowid,text) VALUES(new.rowid,new.text); END;
            CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
                INSERT INTO messages_fts(messages_fts,rowid,text) VALUES('delete',old.rowid,old.text); END;
            CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
                INSERT INTO messages_fts(messages_fts,rowid,text) VALUES('delete',old.rowid,old.text);
                INSERT INTO messages_fts(rowid,text) VALUES(new.rowid,new.text); END;
            CREATE TABLE IF NOT EXISTS archive_events (id INTEGER PRIMARY KEY, occurred_ms INTEGER NOT NULL, kind TEXT NOT NULL, details TEXT NOT NULL);
            PRAGMA user_version=1;").map_err(|_| Error::storage())?;
        Ok(Self {
            connection,
            scope,
            _lock: lock,
        })
    }

    pub fn parse_jsonl(bytes: &[u8]) -> Result<Vec<Message>> {
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err(Error::invalid("Normalized import exceeds 32 MiB"));
        }
        let mut messages = Vec::new();
        for line in bytes.split(|b| *b == b'\n') {
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if line.len() > 1024 * 1024 {
                return Err(Error::invalid("A normalized JSONL record exceeds 1 MiB"));
            }
            messages.push(serde_json::from_slice(line).map_err(|_| Error::invalid("Invalid normalized JSONL record; message content and parse details are redacted"))?);
            if messages.len() > MAX_IMPORT_MESSAGES {
                return Err(Error::invalid(
                    "Import is limited to 10000 records per invocation",
                ));
            }
        }
        if messages.is_empty() {
            return Err(Error::invalid(
                "Normalized JSONL input contains no messages",
            ));
        }
        Ok(messages)
    }

    pub fn import(
        &mut self,
        mut messages: Vec<Message>,
        cutoff: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<ImportReport> {
        if messages.len() > MAX_IMPORT_MESSAGES {
            return Err(Error::invalid("Import is limited to 10000 records"));
        }
        for message in &mut messages {
            message.validate()?;
            if message.scope != self.scope {
                return Err(Error::new(
                    ErrorCode::ScopeConflict,
                    "A message belongs to another account or tenant; the entire import was rejected",
                ));
            }
            message.normalize_import();
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| Error::storage())?;
        let mut report = ImportReport::default();
        for message in messages {
            if cutoff.is_some_and(|t| message.sent_at < t) {
                report.outside_retention += 1;
                continue;
            }
            let json = serde_json::to_string(&message).map_err(|_| Error::storage())?;
            if json.len() > 1024 * 1024 {
                return Err(Error::invalid("A normalized archive record exceeds 1 MiB"));
            }
            let existing: Option<(i64, String)> = transaction
                .query_row(
                    "SELECT revision_ms,json FROM messages WHERE chat_id=?1 AND message_id=?2",
                    params![message.chat_id, message.message_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|_| Error::storage())?;
            match existing {
                Some((_, previous)) if previous == json => {
                    report.duplicates += 1;
                    continue;
                }
                Some((revision, _)) if revision > message.revision_ms() => {
                    report.stale += 1;
                    continue;
                }
                Some((revision, _)) if revision == message.revision_ms() => {
                    return Err(Error::new(
                        ErrorCode::ArchiveConflict,
                        "Conflicting records have the same message identity and revision; the entire import was rolled back",
                    ));
                }
                Some(_) => report.updated += 1,
                None => report.inserted += 1,
            }
            transaction.execute("INSERT INTO messages(chat_id,message_id,sent_ms,revision_ms,thread_id,text,json) VALUES(?1,?2,?3,?4,?5,?6,?7)
                ON CONFLICT(chat_id,message_id) DO UPDATE SET sent_ms=excluded.sent_ms,revision_ms=excluded.revision_ms,thread_id=excluded.thread_id,text=excluded.text,json=excluded.json",
                params![message.chat_id,message.message_id,message.sent_at.timestamp_millis(),message.revision_ms(),message.thread_id,message.text.as_deref().unwrap_or(""),json]).map_err(|_| Error::storage())?;
        }
        if let Some(cutoff) = cutoff {
            let removed = transaction
                .execute(
                    "DELETE FROM messages WHERE sent_ms < ?1",
                    [cutoff.timestamp_millis()],
                )
                .map_err(|_| Error::storage())?;
            if removed > 0 {
                transaction.execute("INSERT INTO archive_events(occurred_ms,kind,details) VALUES(?1,'retention_prune',?2)", params![now.timestamp_millis(),cutoff.to_rfc3339()]).map_err(|_| Error::storage())?;
            }
        }
        transaction.execute("INSERT INTO archive_events(occurred_ms,kind,details) VALUES(?1,'normalized_import','unverified_source;contiguous_coverage_unknown')", [now.timestamp_millis()]).map_err(|_| Error::storage())?;
        transaction.commit().map_err(|_| Error::storage())?;
        report.completeness = Some(Completeness::local());
        Ok(report)
    }

    pub fn messages(&self, query: &MessageQuery) -> Result<Page<Message>> {
        validate_limit(query.limit)?;
        if let Some(id) = &query.chat {
            validate_id(id)?;
        }
        if let Some(id) = &query.thread {
            validate_id(id)?;
        }
        if let Some(search) = &query.search
            && (search.trim().is_empty() || search.len() > 1024)
        {
            return Err(Error::invalid("Local search requires 1 to 1024 bytes"));
        }
        let binding = binding(&(&self.scope, query))?;
        let cursor = decode_cursor(query.cursor.as_deref(), &binding)?;
        let search_clause = if query.search.is_some() {
            "m.rowid IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?4)"
        } else {
            "1"
        };
        let sql = format!("SELECT m.json FROM messages m WHERE (?1 IS NULL OR m.chat_id=?1) AND (?2 IS NULL OR m.thread_id=?2)
            AND m.sent_ms>=?3 AND {search_clause} AND (?5=0 OR (m.sent_ms,m.message_id,m.chat_id)<(?6,?7,?8))
            ORDER BY m.sent_ms DESC,m.message_id DESC,m.chat_id DESC LIMIT ?9");
        let phrase = query
            .search
            .as_ref()
            .map(|s| format!("\"{}\"", s.replace('"', "\"\"")));
        let mut statement = self
            .connection
            .prepare(&sql)
            .map_err(|_| Error::storage())?;
        let records = statement
            .query_map(
                params![
                    query.chat,
                    query.thread,
                    query
                        .since
                        .map(|t| t.timestamp_millis())
                        .unwrap_or(i64::MIN),
                    phrase,
                    cursor.is_some(),
                    cursor.as_ref().map(|c| c.sent_ms).unwrap_or(0),
                    cursor.as_ref().map(|c| c.id.as_str()).unwrap_or(""),
                    cursor.as_ref().map(|c| c.chat.as_str()).unwrap_or(""),
                    (query.limit + 1) as i64
                ],
                |r| r.get::<_, String>(0),
            )
            .map_err(|_| Error::storage())?;
        let mut items = Vec::new();
        for record in records {
            items.push(
                serde_json::from_str::<Message>(&record.map_err(|_| Error::storage())?)
                    .map_err(|_| Error::storage())?,
            );
        }
        let has_more = items.len() > query.limit;
        items.truncate(query.limit);
        let next_cursor = if has_more {
            items
                .last()
                .map(|m| {
                    encode_cursor(Cursor {
                        binding,
                        sent_ms: m.sent_at.timestamp_millis(),
                        id: m.message_id.clone(),
                        chat: m.chat_id.clone(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        Ok(Page {
            items,
            source: "local_archive",
            limit: query.limit,
            has_more,
            next_cursor,
            completeness: Completeness::local(),
        })
    }

    pub fn get_message(&self, id: &str) -> Result<Message> {
        validate_id(id)?;
        let mut statement = self
            .connection
            .prepare("SELECT json FROM messages WHERE message_id=?1 LIMIT 2")
            .map_err(|_| Error::storage())?;
        let records = statement
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|_| Error::storage())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| Error::storage())?;
        match records.as_slice() {
            [] => Err(Error::new(
                ErrorCode::NotFound,
                "Message is absent from the local archive",
            )),
            [one] => serde_json::from_str(one).map_err(|_| Error::storage()),
            _ => Err(Error::new(
                ErrorCode::AmbiguousId,
                "Message ID occurs in multiple archived chats; use messages list --chat to disambiguate",
            )),
        }
    }

    pub fn chats(&self, limit: usize, cursor: Option<&str>) -> Result<Page<Chat>> {
        validate_limit(limit)?;
        let binding = binding(&(&self.scope, "chats"))?;
        let cursor = decode_cursor(cursor, &binding)?;
        let mut statement = self.connection.prepare("SELECT chat_id,count(*),max(sent_ms) FROM messages WHERE (?1 IS NULL OR chat_id>?1) GROUP BY chat_id ORDER BY chat_id LIMIT ?2").map_err(|_| Error::storage())?;
        let records = statement
            .query_map(
                params![cursor.as_ref().map(|c| c.id.as_str()), (limit + 1) as i64],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                },
            )
            .map_err(|_| Error::storage())?;
        let mut items = Vec::new();
        for record in records {
            let (chat_id, count, time) = record.map_err(|_| Error::storage())?;
            items.push(Chat {
                scope: self.scope.clone(),
                chat_id,
                name: None,
                kind: "unknown",
                message_count: count_usize(count)?,
                last_message_at: datetime(time)?,
                discovery: "known_from_local_messages",
            });
        }
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = if has_more {
            items
                .last()
                .map(|c| {
                    encode_cursor(Cursor {
                        binding,
                        sent_ms: 0,
                        id: c.chat_id.clone(),
                        chat: String::new(),
                    })
                })
                .transpose()?
        } else {
            None
        };
        Ok(Page {
            items,
            source: "local_archive",
            limit,
            has_more,
            next_cursor,
            completeness: Completeness::local(),
        })
    }

    pub fn get_chat(&self, id: &str) -> Result<Chat> {
        validate_id(id)?;
        let (count, time): (i64, Option<i64>) = self
            .connection
            .query_row(
                "SELECT count(*),max(sent_ms) FROM messages WHERE chat_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| Error::storage())?;
        let time = time.ok_or_else(|| {
            Error::new(ErrorCode::NotFound, "Chat is absent from the local archive")
        })?;
        Ok(Chat {
            scope: self.scope.clone(),
            chat_id: id.to_owned(),
            name: None,
            kind: "unknown",
            message_count: count_usize(count)?,
            last_message_at: datetime(time)?,
            discovery: "known_from_local_messages",
        })
    }

    pub fn coverage(&self, chat: Option<&str>) -> Result<Vec<Coverage>> {
        let mut statement = self.connection.prepare("SELECT chat_id,min(sent_ms),max(sent_ms),count(*) FROM messages WHERE (?1 IS NULL OR chat_id=?1) GROUP BY chat_id ORDER BY chat_id LIMIT 1001").map_err(|_| Error::storage())?;
        let records = statement
            .query_map([chat], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|_| Error::storage())?;
        let mut coverage = Vec::new();
        let deletion_events: i64 = self.connection.query_row("SELECT count(*) FROM archive_events WHERE kind IN ('retention_prune','explicit_prune','clear')", [], |r| r.get(0)).map_err(|_| Error::storage())?;
        for record in records {
            let (chat_id, first, last, count) = record.map_err(|_| Error::storage())?;
            let mut gaps = vec![
                "unverified_import_source".into(),
                "contiguous_history_unknown".into(),
                "server_retention_and_deletions_not_observed".into(),
            ];
            if deletion_events > 0 {
                gaps.push("local_deletion_or_retention_applied".into());
            }
            coverage.push(Coverage {
                chat_id,
                observed_first_message_at: Some(datetime(first)?),
                observed_last_message_at: Some(datetime(last)?),
                retained_message_count: count_usize(count)?,
                contiguous_history_verified: false,
                gaps,
            });
        }
        if coverage.len() > 1000 {
            return Err(Error::invalid(
                "Coverage is limited to 1000 chats; select --chat",
            ));
        }
        Ok(coverage)
    }

    pub fn prune(&mut self, before: DateTime<Utc>, now: DateTime<Utc>) -> Result<usize> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| Error::storage())?;
        let count = transaction
            .execute(
                "DELETE FROM messages WHERE sent_ms<?1",
                [before.timestamp_millis()],
            )
            .map_err(|_| Error::storage())?;
        transaction.execute("INSERT INTO archive_events(occurred_ms,kind,details) VALUES(?1,'explicit_prune',?2)", params![now.timestamp_millis(),before.to_rfc3339()]).map_err(|_| Error::storage())?;
        transaction.commit().map_err(|_| Error::storage())?;
        self.compact()?;
        Ok(count)
    }

    pub fn clear(&mut self, now: DateTime<Utc>) -> Result<usize> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| Error::storage())?;
        let count = transaction
            .execute("DELETE FROM messages", [])
            .map_err(|_| Error::storage())?;
        transaction
            .execute("DELETE FROM archive_events", [])
            .map_err(|_| Error::storage())?;
        transaction.execute("INSERT INTO archive_events(occurred_ms,kind,details) VALUES(?1,'clear','all_local_messages_removed')", [now.timestamp_millis()]).map_err(|_| Error::storage())?;
        transaction.commit().map_err(|_| Error::storage())?;
        self.compact()?;
        Ok(count)
    }

    fn compact(&self) -> Result<()> {
        self.connection
            .execute_batch("INSERT INTO messages_fts(messages_fts) VALUES('rebuild'); VACUUM;")
            .map_err(|_| Error::storage())
    }
}

fn datetime(ms: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).ok_or_else(Error::storage)
}
fn count_usize(count: i64) -> Result<usize> {
    usize::try_from(count).map_err(|_| Error::storage())
}

fn validate_limit(limit: usize) -> Result<()> {
    if !(1..=1000).contains(&limit) {
        return Err(Error::invalid("Limit must be 1 to 1000"));
    }
    Ok(())
}

fn binding<T: Serialize>(value: &T) -> Result<String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|_| Error::storage())?,
    )))
}

fn encode_cursor(cursor: Cursor) -> Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).map_err(|_| Error::storage())?))
}

fn decode_cursor(value: Option<&str>, binding: &str) -> Result<Option<Cursor>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let invalid = || {
        Error::new(
            ErrorCode::InvalidCursor,
            "Cursor is malformed or belongs to a different scope/filter/limit",
        )
    };
    if value.len() > 4096 {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| invalid())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.binding != binding || cursor.id.len() > 256 || cursor.chat.len() > 256 {
        return Err(invalid());
    }
    Ok(Some(cursor))
}
